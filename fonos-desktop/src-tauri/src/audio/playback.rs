//! Audio playback: buffers and plays back TTS audio received from Fonos API.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};

use crate::audio::ref_envelope::RefEnvelope;

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
                env.begin();
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
        if empty {
            // Queue drained: drop the stale timeline and report silence.
            env.reset();
            return 0.0;
        }
        // The queue is still playing; the envelope's own position query returns
        // 0 if it happens to hold no anchored blocks.
        env.reference_rms()
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
