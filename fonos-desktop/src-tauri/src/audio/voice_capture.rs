//! Voice-processed full-duplex audio via the bundled `fonos-voice-capture`
//! Swift helper (macOS).
//!
//! The helper runs an `AVAudioEngine` with voice-processing I/O enabled on
//! BOTH nodes: mic capture and the assistant's TTS playback share one engine,
//! so Apple's echo canceller sees the true playback signal as its cancellation
//! reference and genuinely subtracts the assistant's voice from the mic.
//! (Played anywhere else — e.g. rodio in this process — the VPIO reference is
//! silence and nothing is cancelled; system-wide it would only *duck* other
//! audio, which the helper disables to keep TTS volume.)
//!
//! Capture: the helper writes raw 16 kHz mono Int16 PCM to stdout, exactly
//! like [`crate::audio::system_capture::SystemAudioCapture`]; a background
//! reader thread drains that into a ring buffer.
//!
//! Playback: [`VoiceProcessedCapture::play_pcm`] writes little-endian
//! `[u32 len][len bytes of 16 kHz mono i16 PCM]` frames to the helper's stdin;
//! a zero-length frame is the FLUSH control (barge cut / hangup). The helper
//! answers on stderr with line-oriented controls — `READY` once the engine
//! runs, `DRAIN` whenever the playback queue empties — which a stderr reader
//! thread folds into [`VoiceProcessedCapture::wait_drained`]; all other stderr
//! lines are forwarded to this process's logs.
//!
//! Used by call mode (`call_barge_in` enabled) so the mic can stay hot while
//! the reply plays without the assistant's voice bleeding in and
//! self-triggering barge-in. The regular dictation path stays on cpal and is
//! untouched.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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

/// Playback-side link to the helper: the stdin frame pipe plus the
/// READY/DRAIN state fed by the stderr reader. Shared (via `Arc`) between
/// [`VoiceProcessedCapture`] and any [`HelperAudioOut`] handed to the STS
/// pipeline, so playback outlives neither.
struct PlaybackLink {
    /// Helper stdin; `None` until started or after the pipe is torn down.
    stdin: Mutex<Option<ChildStdin>>,
    state: Mutex<PlaybackFlags>,
    cond: Condvar,
}

#[derive(Default)]
struct PlaybackFlags {
    /// Helper engine confirmed running (`READY` seen on stderr).
    ready: bool,
    /// Audio scheduled since the last DRAIN/FLUSH — i.e. "playback live".
    /// Set on each [`PlaybackLink::play_pcm`], cleared by a subsequent `DRAIN`
    /// (or a flush). A DRAIN that raced an in-flight `play_pcm` and arrived
    /// *before* it clears the older pending; the new play re-arms the flag —
    /// the "late DRAIN" handling.
    pending: bool,
}

impl PlaybackLink {
    fn new() -> Self {
        Self {
            stdin: Mutex::new(None),
            state: Mutex::new(PlaybackFlags::default()),
            cond: Condvar::new(),
        }
    }

    /// Send one playback frame (16 kHz mono i16 PCM bytes) to the helper.
    fn play_pcm(&self, pcm: &[u8]) -> Result<(), String> {
        // Mark pending BEFORE the bytes hit the pipe so a (theoretical)
        // instant DRAIN can never race ahead of the flag.
        self.state.lock().map_err(|e| e.to_string())?.pending = true;
        let write = (|| -> std::io::Result<()> {
            let mut guard = self
                .stdin
                .lock()
                .map_err(|_| std::io::Error::other("stdin lock poisoned"))?;
            let stdin = guard
                .as_mut()
                .ok_or_else(|| std::io::Error::other("helper stdin not open"))?;
            stdin.write_all(&(pcm.len() as u32).to_le_bytes())?;
            stdin.write_all(pcm)?;
            stdin.flush()
        })();
        if let Err(e) = write {
            if let Ok(mut st) = self.state.lock() {
                st.pending = false;
                self.cond.notify_all();
            }
            return Err(format!("fonos-voice-capture playback write failed: {e}"));
        }
        Ok(())
    }

    /// Send the zero-length FLUSH control frame: the helper stops the player
    /// and discards everything scheduled. Clears `pending` locally — there is
    /// nothing left to wait for.
    fn flush(&self) {
        if let Ok(mut guard) = self.stdin.lock() {
            if let Some(stdin) = guard.as_mut() {
                let _ = stdin.write_all(&0u32.to_le_bytes());
                let _ = stdin.flush();
            }
        }
        if let Ok(mut st) = self.state.lock() {
            st.pending = false;
            self.cond.notify_all();
        }
    }

    /// Block until everything played (a `DRAIN` arrived after the most recent
    /// `play_pcm`), or `timeout` elapsed. `true` = drained.
    fn wait_drained(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let Ok(mut st) = self.state.lock() else {
            return false;
        };
        while st.pending {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            match self.cond.wait_timeout(st, deadline - now) {
                Ok((guard, _)) => st = guard,
                Err(_) => return false,
            }
        }
        true
    }

    /// Whether playback audio is scheduled/playing right now.
    fn is_playing(&self) -> bool {
        self.state.lock().map(|st| st.pending).unwrap_or(false)
    }

    /// Block until the helper reports `READY` (engine running) or `timeout`
    /// elapses. `true` = ready.
    fn wait_ready(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let Ok(mut st) = self.state.lock() else {
            return false;
        };
        while !st.ready {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            match self.cond.wait_timeout(st, deadline - now) {
                Ok((guard, _)) => st = guard,
                Err(_) => return false,
            }
        }
        true
    }

    /// Fold one helper stderr line into the playback state; non-control lines
    /// are the helper's logs and are forwarded by the caller.
    fn on_stderr_line(&self, line: &str) -> bool {
        match line {
            "READY" => {
                if let Ok(mut st) = self.state.lock() {
                    st.ready = true;
                    self.cond.notify_all();
                }
                true
            }
            "DRAIN" => {
                if let Ok(mut st) = self.state.lock() {
                    st.pending = false;
                    self.cond.notify_all();
                }
                true
            }
            _ => false,
        }
    }
}

/// Voice-processed (echo-cancelled) full-duplex audio backed by the
/// `fonos-voice-capture` Swift helper.
///
/// Capture mirrors the [`crate::audio::capture::AudioCapture`] API the call
/// loop uses — `start` / `stop` / `take_chunk(ms)` — so the loop can drain
/// from either source behind a small enum. `take_chunk` takes `&self`
/// (interior mutability) so the same instance can back both the listen phase
/// and the barge monitor. Playback adds `play_pcm` / `flush_playback` /
/// `wait_drained`, plus [`audio_out`](Self::audio_out) to hand the STS
/// pipeline an [`fonos_core::sts::AudioOut`] that routes TTS through the
/// helper's engine.
pub struct VoiceProcessedCapture {
    child: Option<Child>,
    buffer: Arc<Mutex<CaptureBuffer>>,
    _reader: Option<thread::JoinHandle<()>>,
    _stderr_reader: Option<thread::JoinHandle<()>>,
    link: Arc<PlaybackLink>,
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
            _stderr_reader: None,
            link: Arc::new(PlaybackLink::new()),
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
            .stdin(Stdio::piped()) // playback frames
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()) // READY/DRAIN controls + forwarded logs
            .spawn()
            .map_err(|e| format!("failed to spawn fonos-voice-capture: {e}"))?;

        let stdout: ChildStdout = child
            .stdout
            .take()
            .ok_or("failed to get stdout from fonos-voice-capture")?;
        let stderr: ChildStderr = child
            .stderr
            .take()
            .ok_or("failed to get stderr from fonos-voice-capture")?;
        if let Ok(mut guard) = self.link.stdin.lock() {
            *guard = child.stdin.take();
        }

        let buffer = Arc::clone(&self.buffer);
        let handle = thread::spawn(move || reader_thread(stdout, buffer));
        let link = Arc::clone(&self.link);
        let stderr_handle = thread::spawn(move || stderr_thread(stderr, link));

        self.child = Some(child);
        self._reader = Some(handle);
        self._stderr_reader = Some(stderr_handle);
        Ok(())
    }

    /// Kill the helper and clear the ring buffer.
    pub fn stop(&mut self) {
        // Close the playback pipe first so the helper isn't killed mid-frame.
        if let Ok(mut guard) = self.link.stdin.lock() {
            *guard = None;
        }
        if let Ok(mut st) = self.link.state.lock() {
            st.pending = false;
            st.ready = false;
            self.link.cond.notify_all();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self._reader = None;
        self._stderr_reader = None;
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

    /// Schedule one frame of 16 kHz mono i16 PCM bytes for playback through
    /// the helper's engine (the VPIO echo-cancellation reference).
    #[allow(dead_code)] // exercised via HelperAudioOut::push (shared link)
    pub fn play_pcm(&self, pcm: &[u8]) -> Result<(), String> {
        self.link.play_pcm(pcm)
    }

    /// Cut playback NOW: the helper stops its player and discards everything
    /// scheduled (barge interrupt / hangup).
    pub fn flush_playback(&self) {
        self.link.flush();
    }

    /// Block until the helper reports its playback queue drained (a `DRAIN`
    /// after the most recent [`play_pcm`](Self::play_pcm)) or `timeout`
    /// elapses. `true` = drained.
    #[allow(dead_code)] // exercised via HelperAudioOut::finish
    pub fn wait_drained(&self, timeout: Duration) -> bool {
        self.link.wait_drained(timeout)
    }

    /// Whether helper playback is currently live (scheduled audio not yet
    /// drained) — the call loop's "reply is audible" signal in this mode.
    pub fn is_playing(&self) -> bool {
        self.link.is_playing()
    }

    /// Block until the helper's engine is confirmed running (`READY` on
    /// stderr) or `timeout` elapses — the engage gate for call mode, so a
    /// helper that spawned but failed VPIO/engine setup falls back cleanly.
    pub fn wait_ready(&self, timeout: Duration) -> bool {
        self.link.wait_ready(timeout)
    }

    /// An [`fonos_core::sts::AudioOut`] that plays TTS through the helper's
    /// engine. Shares the playback link, so barge flushes and drain waits stay
    /// coherent with this capture.
    pub fn audio_out(&self) -> HelperAudioOut {
        HelperAudioOut {
            link: Arc::clone(&self.link),
            format: Mutex::new((16_000, 1)),
            pushed_ms: AtomicU64::new(0),
        }
    }
}

impl Drop for VoiceProcessedCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Streams synthesized TTS into the voice helper's playback path — the
/// call-mode counterpart of [`crate::adapters::PlaybackAudioOut`]. TTS servers
/// return WAV at their own rate (24 kHz for Kokoro/Qwen); the helper expects
/// 16 kHz mono i16, so each chunk is downmixed and resampled
/// ([`fonos_core::audio::resample_i16`]) before it crosses the pipe.
pub struct HelperAudioOut {
    link: Arc<PlaybackLink>,
    /// Stream format from `begin` (server sample rate, channels).
    format: Mutex<(u32, u16)>,
    /// Total audio pushed since `begin`, in ms at 16 kHz — sizes the drain
    /// timeout in [`finish`](fonos_core::sts::AudioOut::finish).
    pushed_ms: AtomicU64,
}

impl fonos_core::tts::PcmSink for HelperAudioOut {
    fn begin(&self, sample_rate: u32, channels: u16) -> Result<(), String> {
        *self.format.lock().map_err(|e| e.to_string())? = (sample_rate, channels);
        self.pushed_ms.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn push(&self, pcm: &[u8]) -> Result<(), String> {
        let (rate, channels) = *self.format.lock().map_err(|e| e.to_string())?;
        let mut samples: Vec<i16> = pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        // Downmix interleaved multi-channel to mono by averaging frames.
        if channels > 1 {
            samples = samples
                .chunks(channels as usize)
                .map(|frame| {
                    (frame.iter().map(|&s| s as i32).sum::<i32>() / frame.len() as i32) as i16
                })
                .collect();
        }
        let samples = fonos_core::audio::resample_i16(&samples, rate, 16_000);
        if samples.is_empty() {
            return Ok(());
        }
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        self.link.play_pcm(&bytes)?;
        self.pushed_ms
            .fetch_add(samples.len() as u64 / 16, Ordering::SeqCst); // 16 samples/ms
        Ok(())
    }
}

#[async_trait::async_trait]
impl fonos_core::sts::AudioOut for HelperAudioOut {
    async fn finish(&self) -> Result<(), String> {
        // Sane bound: everything pushed, plus headroom for scheduling latency.
        // (A barge flush clears `pending`, so an interrupted reply returns
        // immediately rather than waiting out the discarded tail.)
        let timeout = Duration::from_millis(self.pushed_ms.load(Ordering::SeqCst) + 5_000);
        let link = Arc::clone(&self.link);
        let drained = tokio::task::spawn_blocking(move || link.wait_drained(timeout))
            .await
            .map_err(|e| format!("drain wait task failed: {e}"))?;
        if drained {
            Ok(())
        } else {
            Err("voice helper playback did not drain in time".to_string())
        }
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

/// Drain the helper's stderr: `READY`/`DRAIN` control lines update the
/// playback link; everything else is the helper's own logging, forwarded to
/// this process's stderr (the helper prefixes its lines itself).
fn stderr_thread(stderr: ChildStderr, link: Arc<PlaybackLink>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if !link.on_stderr_line(line.trim_end()) {
            eprintln!("{line}");
        }
    }
    // Helper exited (or stderr closed): unblock any drain waiter.
    if let Ok(mut st) = link.state.lock() {
        st.pending = false;
        st.ready = false;
        link.cond.notify_all();
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
    // CWD-relative dev paths: `cargo tauri dev` may run with the working
    // directory at the crate root (`fonos-desktop/`) or the repo root.
    candidates.push(std::path::PathBuf::from(format!("src-tauri/resources/{name}")));
    candidates.push(std::path::PathBuf::from(format!(
        "fonos-desktop/src-tauri/resources/{name}"
    )));
    // Absolute path into the source tree — always resolves under `cargo tauri
    // dev` regardless of the working directory (CARGO_MANIFEST_DIR is this
    // crate's dir, `.../fonos-desktop/src-tauri`). In a bundled release the
    // baked-in build-machine path simply won't exist, so it's skipped.
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
