//! Audio subsystem: CoreAudio mic capture, PCM playback, and dual-channel
//! meeting capture via ScreenCaptureKit.

pub mod capture;
pub mod playback;
pub mod system_capture;
pub mod dual_capture;

/// macOS voice-processing (VPIO) mic capture helper — system echo cancellation
/// for call mode. Only built on macOS (spawns the `fonos-voice-capture` helper).
#[cfg(target_os = "macos")]
pub mod voice_capture;

/// Linux system echo cancellation via PulseAudio/PipeWire `module-echo-cancel`.
#[cfg(target_os = "linux")]
pub mod linux_aec;
