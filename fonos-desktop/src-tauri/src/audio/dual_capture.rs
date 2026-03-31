//! Dual-channel audio capture: microphone (left/me) + system audio (right/others).
//!
//! In remote-meeting mode both channels are captured simultaneously and returned
//! as a `DualChunk`.  In mic-only mode (face-to-face meetings, or when SCK is
//! not available) `system_samples` is `None`.

use crate::audio::capture::AudioCapture;
use crate::audio::system_capture::SystemAudioCapture;

/// A single chunk of dual-channel audio.
pub struct DualChunk {
    /// Microphone samples (16 kHz, 16-bit mono) — the local speaker ("Me").
    pub mic_samples: Vec<i16>,
    /// System audio samples (16 kHz, 16-bit mono) — remote participants ("Others").
    /// `None` when operating in mic-only / single-channel mode.
    pub system_samples: Option<Vec<i16>>,
}

/// Manages simultaneous microphone + system audio capture for meeting mode.
pub struct DualCapture {
    mic: AudioCapture,
    system: Option<SystemAudioCapture>,
}

impl DualCapture {
    /// Create a new `DualCapture`.
    ///
    /// Attempts to also create a `SystemAudioCapture`; if that fails (e.g.
    /// ScreenCaptureKit not available or permission denied) the capture falls
    /// back to mic-only mode gracefully.
    pub fn new() -> Result<Self, String> {
        let mic = AudioCapture::new().map_err(|e| format!("mic init failed: {e}"))?;

        let system = if SystemAudioCapture::is_available() {
            match SystemAudioCapture::new() {
                Ok(s) => {
                    eprintln!("fonos: DualCapture — system audio (SCK) available");
                    Some(s)
                }
                Err(e) => {
                    eprintln!("fonos: DualCapture — system audio unavailable ({e}), mic-only");
                    None
                }
            }
        } else {
            eprintln!("fonos: DualCapture — ScreenCaptureKit not available, mic-only");
            None
        };

        Ok(Self { mic, system })
    }

    /// Start both capture channels.
    ///
    /// Mic failure is fatal; system audio failure just drops back to mono.
    pub fn start(&mut self) -> Result<(), String> {
        self.mic.start().map_err(|e| format!("mic start failed: {e}"))?;

        if let Some(sys) = self.system.as_mut() {
            if let Err(e) = sys.start() {
                eprintln!("fonos: DualCapture — system audio start failed ({e}), continuing mic-only");
                self.system = None;
            }
        }

        Ok(())
    }

    /// Stop both capture channels.
    pub fn stop(&mut self) {
        self.mic.stop();
        if let Some(sys) = self.system.as_mut() {
            sys.stop();
        }
    }

    /// Returns `true` when both mic AND system audio are active (dual-channel mode).
    pub fn is_dual_channel(&self) -> bool {
        self.system.is_some()
    }

    /// Take the next chunk of audio from both channels.
    ///
    /// `timeout_ms` controls how long (roughly) to wait for the mic buffer to
    /// accumulate enough samples before returning `None`.  Each call drains
    /// `timeout_ms` worth of audio from the mic ring buffer (at 16 kHz this is
    /// `16 * timeout_ms` samples).
    ///
    /// Returns `None` if not enough mic audio has been buffered yet.
    pub fn take_chunk(&mut self, timeout_ms: u64) -> Option<DualChunk> {
        // Drain mic audio — `take_chunk` on AudioCapture takes `duration_ms: u32`
        let mic_samples = match self.mic.take_chunk(timeout_ms as u32) {
            Some(s) => s,
            None => {
                // Mic doesn't have enough data yet — still drain system audio
                // into a chunk with empty mic so callers can accumulate both
                // channels independently without losing system audio.
                let system_samples = self.system.as_mut().and_then(|s| s.take_chunk(timeout_ms));
                if system_samples.is_some() {
                    return Some(DualChunk {
                        mic_samples: Vec::new(),
                        system_samples,
                    });
                }
                return None;
            }
        };

        // Drain system audio if available (best-effort; may return None if stub or silent).
        let system_samples = self.system.as_mut().and_then(|s| s.take_chunk(timeout_ms));

        Some(DualChunk {
            mic_samples,
            system_samples,
        })
    }
}
