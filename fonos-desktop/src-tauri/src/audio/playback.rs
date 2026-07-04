//! Audio playback: buffers and plays back TTS audio received from Fonos API.

use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

/// Represents the current state of audio playback.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackState {
    Idle,
    Playing,
    Paused,
    Stopped,
}

/// Errors that can occur during audio playback.
#[derive(Debug)]
pub enum PlaybackError {
    /// Failed to open the audio output stream.
    StreamError(String),
    /// Failed to create a playback sink.
    SinkError(String),
    /// Failed to decode the provided WAV data.
    DecodeError(String),
}

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlaybackError::StreamError(msg) => write!(f, "Stream error: {}", msg),
            PlaybackError::SinkError(msg) => write!(f, "Sink error: {}", msg),
            PlaybackError::DecodeError(msg) => write!(f, "Decode error: {}", msg),
        }
    }
}

impl std::error::Error for PlaybackError {}

/// Thread-safe audio playback controller backed by rodio.
///
/// The `OutputStream` must be kept alive for the duration of playback — it is
/// stored inside the struct alongside the `Sink`. Both are protected behind an
/// `Arc<Mutex<...>>` so that `AudioPlayback` itself can be `Clone`d and shared
/// across threads (e.g. passed to a Tauri command handler).
pub struct AudioPlayback {
    state: Arc<Mutex<PlaybackState>>,
    inner: Arc<Mutex<PlaybackInner>>,
    /// Live loudness timeline of the audio we have pushed, used by the call
    /// loop's barge detector as its playback reference (see [`reference_rms`]).
    ///
    /// [`reference_rms`]: AudioPlayback::reference_rms
    ref_env: Arc<Mutex<RefEnvelope>>,
}

/// Holds the rodio resources that must stay alive while audio is playing.
struct PlaybackInner {
    /// Kept alive so the output stream is not dropped.
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Sink,
}

/// Per-100ms RMS timeline of the audio pushed into the current playback queue.
///
/// We own every PCM chunk we play, so we can record how loud the output *is* on
/// a timeline and, given the wall-clock elapsed since the queue started, answer
/// "how loud is the assistant right now?" — the reference the barge detector
/// gates the mic against. All values are i16 RMS units, matching
/// [`fonos_core::vad::rms`].
#[derive(Default)]
struct RefEnvelope {
    /// `(block_rms, block_ms)` for each ~100 ms block, in play order.
    blocks: VecDeque<(f32, u32)>,
    /// Total duration (ms) represented by `blocks`.
    total_ms: u64,
    /// Wall-clock instant the current queue began playing (first append after
    /// the queue was empty). `None` when idle.
    queue_start: Option<Instant>,
}

/// Target block length for the reference timeline.
const REF_BLOCK_MS: u32 = 100;
/// Half-width (ms) of the window searched around the estimated playback
/// position — wide enough to stay conservative against position error.
const REF_WINDOW_MS: u64 = 400;

impl RefEnvelope {
    /// Drop the whole timeline (queue drained, stopped, or replaced).
    fn reset(&mut self) {
        self.blocks.clear();
        self.total_ms = 0;
        self.queue_start = None;
    }

    /// Append the per-100ms-block RMS of `samples` (interleaved i16 at
    /// `sample_rate` × `channels`) to the timeline.
    fn push_samples(&mut self, samples: &[i16], sample_rate: u32, channels: u16) {
        if samples.is_empty() || sample_rate == 0 || channels == 0 {
            return;
        }
        // Samples per full block (all channels), and the samples-per-ms factor.
        let block_len =
            ((sample_rate as usize * REF_BLOCK_MS as usize / 1000) * channels as usize).max(1);
        let per_ms = (sample_rate as u64 * channels as u64).max(1); // samples/sec·ch → /1000 below
        for block in samples.chunks(block_len) {
            let sum_sq: f64 = block.iter().map(|&s| (s as f64) * (s as f64)).sum();
            let rms = (sum_sq / block.len() as f64).sqrt() as f32;
            let block_ms = ((block.len() as u64 * 1000) / per_ms).max(1) as u32;
            self.blocks.push_back((rms, block_ms));
            self.total_ms += block_ms as u64;
        }
    }

    /// Max block RMS within ±[`REF_WINDOW_MS`] of `pos_ms` along the timeline.
    fn max_rms_around(&self, pos_ms: u64) -> f32 {
        let lo = pos_ms.saturating_sub(REF_WINDOW_MS);
        let hi = pos_ms + REF_WINDOW_MS;
        let mut cursor = 0u64;
        let mut peak = 0.0f32;
        for &(rms, block_ms) in &self.blocks {
            let start = cursor;
            let end = cursor + block_ms as u64;
            // Overlap test between [start, end) and [lo, hi].
            if end > lo && start <= hi && rms > peak {
                peak = rms;
            }
            cursor = end;
        }
        peak
    }
}

// Safety: `OutputStream` and `Sink` are not `Send`/`Sync` on all platforms due
// to platform audio APIs, but we guard all access through `Arc<Mutex<...>>`.
// The stream is never sent across threads raw — only the `Arc` is shared.
unsafe impl Send for AudioPlayback {}
unsafe impl Sync for AudioPlayback {}

#[allow(dead_code)]
impl AudioPlayback {
    /// Create a new `AudioPlayback` instance, opening the default output device.
    pub fn new() -> Result<Self, PlaybackError> {
        let (stream, stream_handle) =
            OutputStream::try_default().map_err(|e| PlaybackError::StreamError(e.to_string()))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| PlaybackError::SinkError(e.to_string()))?;

        Ok(Self {
            state: Arc::new(Mutex::new(PlaybackState::Idle)),
            inner: Arc::new(Mutex::new(PlaybackInner {
                _stream: stream,
                _stream_handle: stream_handle,
                sink,
            })),
            ref_env: Arc::new(Mutex::new(RefEnvelope::default())),
        })
    }

    /// Decode `wav_bytes` and queue them for playback on the default output device.
    ///
    /// Any previously queued audio is cleared before the new audio is appended.
    pub fn play_wav(&self, wav_bytes: Vec<u8>) -> Result<(), PlaybackError> {
        let cursor = Cursor::new(wav_bytes);
        let source =
            Decoder::new(cursor).map_err(|e| PlaybackError::DecodeError(e.to_string()))?;

        let inner = self.inner.lock().unwrap();
        // Clear any previously queued audio before starting fresh.
        inner.sink.stop();
        inner.sink.append(source);
        inner.sink.play();
        drop(inner);
        // `play_wav` is the non-streaming path (not the barge-monitored call
        // loop); we don't track its envelope, but clear any stale timeline so a
        // reference query can't read leftover blocks from a prior stream.
        self.ref_env.lock().unwrap().reset();

        let mut state = self.state.lock().unwrap();
        *state = PlaybackState::Playing;

        Ok(())
    }

    /// Pause playback. Has no effect if already paused or idle.
    pub fn pause(&self) {
        let inner = self.inner.lock().unwrap();
        inner.sink.pause();

        let mut state = self.state.lock().unwrap();
        if *state == PlaybackState::Playing {
            *state = PlaybackState::Paused;
        }
    }

    /// Resume a paused playback. Has no effect if already playing or idle.
    pub fn resume(&self) {
        let inner = self.inner.lock().unwrap();
        inner.sink.play();

        let mut state = self.state.lock().unwrap();
        if *state == PlaybackState::Paused {
            *state = PlaybackState::Playing;
        }
    }

    /// Stop playback and clear the queue.
    pub fn stop(&self) {
        let inner = self.inner.lock().unwrap();
        inner.sink.stop();
        drop(inner);
        self.ref_env.lock().unwrap().reset();

        let mut state = self.state.lock().unwrap();
        *state = PlaybackState::Stopped;
    }

    /// Return a snapshot of the current playback state.
    pub fn state(&self) -> PlaybackState {
        self.state.lock().unwrap().clone()
    }

    /// Append raw 16-bit LE PCM frames to the playback queue (does NOT clear
    /// previously queued audio) and make sure the sink is playing. Used by
    /// the streaming TTS path.
    pub fn append_pcm(&self, sample_rate: u32, channels: u16, pcm: &[u8]) -> Result<(), PlaybackError> {
        let samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        // Record the loudness of this chunk on the reference timeline before the
        // samples are moved into the sink buffer.
        let inner = self.inner.lock().unwrap();
        let was_empty = inner.sink.empty();
        {
            let mut env = self.ref_env.lock().unwrap();
            // A fresh append onto an empty queue starts a new playback timeline.
            if was_empty {
                env.reset();
                env.queue_start = Some(Instant::now());
            }
            env.push_samples(&samples, sample_rate, channels);
        }
        let source = rodio::buffer::SamplesBuffer::new(channels, sample_rate, samples);
        inner.sink.append(source);
        inner.sink.play();
        drop(inner);
        let mut state = self.state.lock().unwrap();
        *state = PlaybackState::Playing;
        Ok(())
    }

    /// Whether the playback queue has drained.
    pub fn queue_empty(&self) -> bool {
        self.inner.lock().unwrap().sink.empty()
    }

    /// Current playback loudness (i16 RMS units), for the call loop's barge
    /// detector to gate the mic against.
    ///
    /// Because we push every PCM chunk ourselves, we know the output's loudness
    /// on a timeline. We estimate the current playback position as the
    /// wall-clock time elapsed since the queue started (clamped to the total
    /// queued duration) and return the MAX block RMS within a ±400 ms window
    /// around it — conservative against position drift, so a momentary
    /// estimation error can never make the reference read *quieter* than the
    /// audio actually reaching the speaker. Returns `0.0` when the queue is
    /// empty or idle (the detector then falls back to its absolute floor).
    pub fn reference_rms(&self) -> f32 {
        // Check emptiness without holding the sink lock across the env lock.
        let empty = self.inner.lock().unwrap().sink.empty();
        let mut env = self.ref_env.lock().unwrap();
        if empty || env.blocks.is_empty() {
            // Queue drained: drop the stale timeline and report silence.
            env.reset();
            return 0.0;
        }
        let Some(start) = env.queue_start else {
            return 0.0;
        };
        let elapsed_ms = start.elapsed().as_millis() as u64;
        let pos_ms = elapsed_ms.min(env.total_ms);
        env.max_rms_around(pos_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the state enum variants are accessible and comparable.
    #[test]
    fn playback_state_variants() {
        assert_eq!(PlaybackState::Idle, PlaybackState::Idle);
        assert_ne!(PlaybackState::Playing, PlaybackState::Paused);
        assert_ne!(PlaybackState::Stopped, PlaybackState::Idle);
    }

    /// Verify that `PlaybackError` types can be formatted.
    #[test]
    fn playback_error_display() {
        let e = PlaybackError::StreamError("test".into());
        assert!(e.to_string().contains("test"));
        let e2 = PlaybackError::DecodeError("bad wav".into());
        assert!(e2.to_string().contains("bad wav"));
    }
}
