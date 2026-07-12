//! System audio capture via ScreenCaptureKit (macOS 13+).
//!
//! This module spawns the bundled `fonos-audio-capture` Swift helper binary,
//! which uses ScreenCaptureKit to record system audio output (loopback capture)
//! and writes raw 16 kHz mono Int16 PCM to its stdout.  A background reader
//! thread drains the child's stdout into a ring buffer that is the same format
//! used by [`crate::audio::capture::AudioCapture`] (mic).
//!
//! # Availability
//! ScreenCaptureKit audio capture requires macOS 13.0+.  On older systems
//! [`SystemAudioCapture::is_available`] returns `false` without spawning anything.
//!
//! # Permissions
//! macOS requires the "Screen Recording" permission for SCK.  The Swift helper
//! will automatically prompt the user on first use.  If permission is denied the
//! helper process exits with an error message on stderr and no PCM output.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Ring buffer capacity: 5 minutes of 16 kHz mono audio.
const RING_CAPACITY: usize = 300 * 16_000;

// ---------------------------------------------------------------------------
// Internal shared state
// ---------------------------------------------------------------------------

struct CaptureBuffer {
    samples: VecDeque<i16>,
}

impl CaptureBuffer {
    fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(RING_CAPACITY),
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) {
        // bytes is raw little-endian Int16 PCM from the Swift helper.
        for chunk in bytes.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            if self.samples.len() >= RING_CAPACITY {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Loopback capture of system audio output via ScreenCaptureKit.
///
/// Uses the bundled `fonos-audio-capture` Swift CLI helper so there is no
/// Objective-C bridging required on the Rust side.
pub struct SystemAudioCapture {
    /// Running child process (Some while capturing).
    child: Option<Child>,
    /// Shared ring buffer fed by the background stdout reader thread.
    buffer: Arc<Mutex<CaptureBuffer>>,
    /// Join handle for the reader thread (kept so we can detect thread death).
    _reader: Option<thread::JoinHandle<()>>,
}

impl SystemAudioCapture {
    /// Check whether ScreenCaptureKit audio capture is available on this
    /// machine.  Runs `fonos-audio-capture check` and parses the JSON result.
    ///
    /// Returns `false` when the helper binary cannot be found or when the
    /// helper reports `available: false` (e.g. macOS < 13, permission denied).
    pub fn is_available() -> bool {
        let Some(binary) = find_audio_capture_binary() else {
            eprintln!("fonos: fonos-audio-capture binary not found");
            return false;
        };

        match Command::new(&binary).arg("check").output() {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                parse_available(&stdout)
            }
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!(
                    "fonos: fonos-audio-capture check failed: {} {}",
                    stdout.trim(),
                    stderr.trim()
                );
                false
            }
            Err(e) => {
                eprintln!("fonos: failed to run fonos-audio-capture check: {e}");
                false
            }
        }
    }

    /// Create a new `SystemAudioCapture`.
    ///
    /// Does **not** start capturing; call [`start`] first.
    pub fn new() -> Result<Self, String> {
        if !Self::is_available() {
            return Err(
                "ScreenCaptureKit audio capture is not available (macOS 13+ required, \
                 or Screen Recording permission not granted)"
                    .to_string(),
            );
        }
        Ok(Self {
            child: None,
            buffer: Arc::new(Mutex::new(CaptureBuffer::new())),
            _reader: None,
        })
    }

    /// Start capturing system audio.
    ///
    /// Spawns `fonos-audio-capture start` and begins draining its stdout into
    /// the internal ring buffer on a background thread.
    pub fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(()); // Already running.
        }

        let binary =
            find_audio_capture_binary().ok_or("fonos-audio-capture binary not found")?;

        let mut child = Command::new(&binary)
            .arg("start")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Let SCK errors appear in Tauri logs.
            .spawn()
            .map_err(|e| format!("failed to spawn fonos-audio-capture: {e}"))?;

        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or("failed to get stdout from fonos-audio-capture")?;

        let buffer = Arc::clone(&self.buffer);

        // Background reader: drains raw PCM bytes from the helper's stdout.
        let handle = thread::spawn(move || {
            reader_thread(stdout, buffer);
        });

        self.child = Some(child);
        self._reader = Some(handle);

        Ok(())
    }

    /// Stop capturing system audio.  Kills the helper process and drops the
    /// ring buffer contents.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self._reader = None;
        if let Ok(mut buf) = self.buffer.lock() {
            buf.samples.clear();
        }
    }

    /// Return up to `timeout_ms` milliseconds of 16 kHz mono i16 audio from
    /// the ring buffer.
    ///
    /// Returns `None` if not enough samples have accumulated yet.  In practice
    /// SCK delivers ~100 ms bursts, so pass at least `100` for `timeout_ms`.
    pub fn take_chunk(&mut self, timeout_ms: u64) -> Option<Vec<i16>> {
        let required = (16_000u64 * timeout_ms / 1000) as usize;
        if required == 0 {
            return None;
        }
        let mut buf = self.buffer.lock().ok()?;
        if buf.samples.len() < required {
            return None;
        }
        let chunk: Vec<i16> = buf.samples.drain(..required).collect();
        Some(chunk)
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Background reader thread
// ---------------------------------------------------------------------------

fn reader_thread(mut stdout: ChildStdout, buffer: Arc<Mutex<CaptureBuffer>>) {
    // Read in 4096-byte chunks (2048 i16 samples = ~128ms at 16kHz).
    let mut raw = [0u8; 4096];
    loop {
        match stdout.read(&mut raw) {
            Ok(0) => {
                eprintln!("fonos: fonos-audio-capture stdout closed (process exited)");
                break;
            }
            Ok(n) => {
                if let Ok(mut buf) = buffer.lock() {
                    buf.push_bytes(&raw[..n]);
                }
            }
            Err(e) => {
                eprintln!("fonos: stdout read error from fonos-audio-capture: {e}");
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: locate the fonos-audio-capture binary
// ---------------------------------------------------------------------------

fn find_audio_capture_binary() -> Option<String> {
    let name = "fonos-audio-capture";

    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    // 1. Next to current executable (covers `cargo run` / debug builds).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            // 2. macOS .app bundle: Contents/MacOS → Contents/Resources/
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("Resources").join(name));
                // 3. Tauri v2 actually nests bundled resources one level
                // deeper: Contents/Resources/resources/ (verified on the
                // installed .app).
                candidates.push(parent.join("Resources").join("resources").join(name));
            }
        }
    }

    // 4. Development paths relative to CWD (for `cargo test` / `cargo run`).
    candidates.push(std::path::PathBuf::from(format!("src-tauri/resources/{name}")));
    candidates.push(std::path::PathBuf::from(format!(
        "fonos-desktop/src-tauri/resources/{name}"
    )));
    // 5. Absolute path into the source tree — always resolves under `cargo
    // tauri dev` regardless of the working directory (CARGO_MANIFEST_DIR is
    // this crate's dir, `.../fonos-desktop/src-tauri`). In a bundled release
    // the baked-in build-machine path simply won't exist, so it's skipped.
    candidates.push(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(name),
    );

    for c in &candidates {
        if c.exists() {
            eprintln!("fonos: found {} at {}", name, c.display());
            return Some(c.to_string_lossy().to_string());
        }
    }

    eprintln!(
        "fonos: {} not found; searched: {:?}",
        name,
        candidates.iter().map(|c| c.display().to_string()).collect::<Vec<_>>()
    );
    None
}

// ---------------------------------------------------------------------------
// Helper: parse {"available": bool, ...} JSON
// ---------------------------------------------------------------------------

fn parse_available(json: &str) -> bool {
    // Minimal parser — avoid adding a serde_json dep just for this one field.
    // The output is a small known structure: {"available":true,"reason":"..."}
    let trimmed = json.trim();
    if let Some(pos) = trimmed.find("\"available\"") {
        let after = &trimmed[pos + 11..]; // skip past "available"
        // Skip whitespace and ':'
        let value_start = after.find(':').map(|i| i + 1).unwrap_or(0);
        let value_str = after[value_start..].trim();
        return value_str.starts_with("true");
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::parse_available;

    #[test]
    fn parse_available_true() {
        assert!(parse_available(r#"{"available":true,"reason":"ok"}"#));
    }

    #[test]
    fn parse_available_false() {
        assert!(!parse_available(r#"{"available":false,"reason":"no"}"#));
    }

    #[test]
    fn parse_available_spaces() {
        assert!(parse_available(r#"{ "available" : true , "reason": "ScreenCaptureKit available" }"#));
    }

    #[test]
    fn parse_available_missing() {
        assert!(!parse_available("{}"));
    }
}
