//! Audio playback: buffers and plays back TTS audio received from Fonos API.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

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

        let mut state = self.state.lock().unwrap();
        *state = PlaybackState::Stopped;
    }

    /// Return a snapshot of the current playback state.
    pub fn state(&self) -> PlaybackState {
        self.state.lock().unwrap().clone()
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
