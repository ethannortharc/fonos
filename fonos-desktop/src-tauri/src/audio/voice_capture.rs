//! Voice-processed microphone capture via the bundled `fonos-voice-capture`
//! Swift helper (macOS).
//!
//! The helper runs an `AVAudioEngine` with voice-processing I/O enabled, so the
//! audio it streams has already had the device's own output (the assistant's
//! TTS) echo-cancelled out, plus noise suppression and AGC applied. It writes
//! raw 16 kHz mono Int16 PCM to stdout, exactly like
//! [`crate::audio::system_capture::SystemAudioCapture`]; a background reader
//! thread drains that into a ring buffer.
//!
//! Used by call mode (`call_barge_in` enabled) so the mic can stay hot while the
//! reply plays without the assistant's voice bleeding in and self-triggering
//! barge-in. The regular dictation path stays on cpal and is untouched.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Ring buffer capacity: 5 minutes of 16 kHz mono audio (matches the mic path).
const RING_CAPACITY: usize = 300 * 16_000;

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
        for chunk in bytes.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            if self.samples.len() >= RING_CAPACITY {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }
}

/// Voice-processed (echo-cancelled) mic capture backed by the
/// `fonos-voice-capture` Swift helper.
///
/// Mirrors the [`crate::audio::capture::AudioCapture`] API the call loop uses —
/// `start` / `stop` / `take_chunk(ms)` — so the loop can drain from either
/// source behind a small enum. `take_chunk` takes `&self` (interior mutability)
/// so the same instance can back both the listen phase and the barge monitor.
pub struct VoiceProcessedCapture {
    child: Option<Child>,
    buffer: Arc<Mutex<CaptureBuffer>>,
    _reader: Option<thread::JoinHandle<()>>,
    /// Mic device name from config (passed to the helper for logging; v1 uses
    /// the system default input regardless).
    device_name: String,
}

impl VoiceProcessedCapture {
    /// Create a new capture bound to `device_name` (may be `"auto"`/`"default"`).
    /// Does not start the helper; call [`start`](Self::start).
    pub fn new(device_name: &str) -> Self {
        Self {
            child: None,
            buffer: Arc::new(Mutex::new(CaptureBuffer::new())),
            _reader: None,
            device_name: device_name.to_string(),
        }
    }

    /// Spawn the helper and begin draining its stdout into the ring buffer.
    ///
    /// Returns an error if the helper binary is missing or cannot be spawned —
    /// the caller falls back to the plain cpal path on error.
    pub fn start(&mut self) -> Result<(), String> {
        if self.child.is_some() {
            return Ok(()); // already running
        }

        let binary =
            find_voice_capture_binary().ok_or("fonos-voice-capture binary not found")?;

        let mut cmd = Command::new(&binary);
        // Pass the configured device name for forward-compat / logging.
        if !self.device_name.is_empty() {
            cmd.arg(&self.device_name);
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // let VPIO/permission errors reach the logs
            .spawn()
            .map_err(|e| format!("failed to spawn fonos-voice-capture: {e}"))?;

        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or("failed to get stdout from fonos-voice-capture")?;

        let buffer = Arc::clone(&self.buffer);
        let handle = thread::spawn(move || reader_thread(stdout, buffer));

        self.child = Some(child);
        self._reader = Some(handle);
        Ok(())
    }

    /// Kill the helper and clear the ring buffer.
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

    /// Return the oldest `timeout_ms` ms of 16 kHz mono i16 audio, or `None` if
    /// not enough has accumulated yet.
    pub fn take_chunk(&self, timeout_ms: u32) -> Option<Vec<i16>> {
        let required = (16_000u64 * timeout_ms as u64 / 1000) as usize;
        if required == 0 {
            return None;
        }
        let mut buf = self.buffer.lock().ok()?;
        if buf.samples.len() < required {
            return None;
        }
        Some(buf.samples.drain(..required).collect())
    }
}

impl Drop for VoiceProcessedCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reader_thread(mut stdout: ChildStdout, buffer: Arc<Mutex<CaptureBuffer>>) {
    let mut raw = [0u8; 4096];
    loop {
        match stdout.read(&mut raw) {
            Ok(0) => {
                eprintln!("fonos: fonos-voice-capture stdout closed (process exited)");
                break;
            }
            Ok(n) => {
                if let Ok(mut buf) = buffer.lock() {
                    buf.push_bytes(&raw[..n]);
                }
            }
            Err(e) => {
                eprintln!("fonos: stdout read error from fonos-voice-capture: {e}");
                break;
            }
        }
    }
}

/// Locate the `fonos-voice-capture` binary — same resolution order as the other
/// bundled helpers: next to the executable, the `.app` Resources dir, then the
/// dev `src-tauri/resources` paths relative to CWD.
fn find_voice_capture_binary() -> Option<String> {
    let name = "fonos-voice-capture";
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(name));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("Resources").join(name));
            }
        }
    }
    candidates.push(std::path::PathBuf::from(format!("src-tauri/resources/{name}")));
    candidates.push(std::path::PathBuf::from(format!(
        "fonos-desktop/src-tauri/resources/{name}"
    )));

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
