//! Platform audio abstraction.
//!
//! Re-exports from `crate::audio::capture` and `crate::audio::playback` so that
//! platform-specific audio code can be accessed through `platform::audio::*`.
//! The actual implementations remain in `audio/` for now.

pub use crate::audio::capture::{AudioCapture, CaptureError};
pub use crate::audio::playback::{AudioPlayback, PlaybackError, PlaybackState};
