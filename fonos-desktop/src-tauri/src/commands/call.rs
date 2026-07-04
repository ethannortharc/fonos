//! Hands-free "call mode" for the STS conversation (issue #24).
//!
//! Where hold-to-talk is one turn per press, call mode runs a continuous loop:
//! listen → detect end-of-utterance (energy VAD, [`fonos_core::vad`]) →
//! transcribe → LLM → spoken reply → listen again, until the user hangs up.
//!
//! The loop is a single background task. It drives capture directly — draining
//! chunks for the VAD and, on [`VadEvent::UtteranceEnd`], transcribing the
//! accumulated buffer through the shared [`transcribe_samples`] path, then
//! running the same [`execute_turn`] pipeline as hold-to-talk.
//!
//! Barge-in (issue #24 v2, gated on `call_barge_in`): while the reply plays,
//! the mic re-opens and a [`barge_monitor`] listens for the user talking over
//! it. Without an AEC the speaker bleeds into the mic, so — rather than trust a
//! static noise floor, which dynamic TTS inevitably overshoots and mistakes for
//! the user — the detector gates the mic against the *live* playback loudness we
//! own ([`AudioPlayback::reference_rms`]): it spends its first ~300 ms learning
//! the speaker→mic coupling, then flags only mic energy that sustains clearly
//! above the bleed expected for whatever is playing at that instant. That fire
//! is a *soft* trigger: rather than cut immediately (AEC residual echo can trip
//! it ~2–3× per 20 turns), the monitor opens a short verify window and confirms
//! only if the energy stays clearly above a raised bar while playback keeps
//! going. On a *confirmed* barge it stops playback, cancels the in-flight turn,
//! and carries the interrupting words straight into the next listen. With
//! barge-in off, the mic stays closed during playback (the original behavior).

use std::collections::VecDeque;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Emitter, Manager};

use fonos_core::config::AppConfig;
use fonos_core::vad::{rms, speech_threshold_factor, BargeDetector, VadConfig, VadEvent, VadSession};

use crate::audio::capture::AudioCapture;
use crate::audio::playback::AudioPlayback;
use super::AppState;
use super::dictation::transcribe_samples;
use super::sts::execute_turn;

/// Chunk size (ms) drained from the ring buffer and fed to the VAD each poll.
const VAD_CHUNK_MS: u32 = 100;
/// Samples per VAD chunk at 16 kHz (100 ms × 16 samples/ms).
const CHUNK_SAMPLES: usize = (VAD_CHUNK_MS as usize) * 16;
/// How long to nap when the ring buffer doesn't yet hold a full chunk.
const POLL_MS: u64 = 30;

/// Barge detector tuning. Warmup lets the detector learn the live playback→mic
/// coupling; a barge is then confirmed only when the mic sustains energy that
/// clearly exceeds the expected bleed for the reference playing right now.
const BARGE_WARMUP_MS: u32 = 300;
const BARGE_SUSTAINED_MS: u32 = 450;
/// Speech-threshold absolute-minimum override (RMS) for the listen VAD on a
/// processed-audio path (system AEC). Post-VPIO/ec silence is near-zero and
/// speech is AGC-levelled far below raw cpal, so the raw-cpal 48–120 clamp —
/// which the relative `noise_floor × factor` bar sits under once the floor
/// adapts down — would leave the call deaf. A small fixed floor lets relative
/// detection dominate while still rejecting flat-line silence. Fallback keeps
/// the raw-cpal ramp (`None`).
const LISTEN_ABS_MIN_AEC: f32 = 12.0;
/// Barge-detector absolute-floor lower bound (RMS) per path. The raw-cpal
/// fallback keeps the conservative 80; AEC paths, whose warmup sees near-silence
/// (peak-bleed term collapses to this), use a low bound and lean on the
/// ambient-seeded floor + reference-coupling instead.
const BARGE_ABS_MIN_AEC: f32 = 15.0;
const BARGE_ABS_MIN_FALLBACK: f32 = 80.0;
/// Pre-roll carried into the next listen so the interrupting words aren't lost
/// (16 000 samples ≈ 1 s at 16 kHz).
const BARGE_PREROLL_SAMPLES: usize = 16_000;
/// Soft-barge verify window: once [`BargeDetector`] fires, keep listening this
/// long for the interruption to prove itself against a *raised* bar before
/// committing to the hard cut. Playback continues untouched throughout —
/// overlap (user and assistant briefly talking at once) is natural, and a real
/// user through AEC runs well above the residual that tripped the soft trigger,
/// so marginal residual bursts can't clear the raised bar. This is what turns
/// the ~2–3 false cuts per 20 turns (AEC residual echo) into no-ops.
const BARGE_VERIFY_WINDOW_MS: u32 = 700;
/// Cumulative time within the verify window that mic energy must exceed the
/// raised bar to CONFIRM a barge; below it the trigger is REFUTED as residual.
const BARGE_VERIFY_VOICED_MS: u32 = 300;
/// Multiplier on the detector's absolute floor forming the verify bar (the
/// `abs_mult` in [`BargeDetector::threshold_for`]). The confirm bar therefore
/// sits clearly above the trigger threshold, while the reference-coupling term
/// (already tracking live playback loudness) is left as-is.
const BARGE_VERIFY_BAR_MULT: f32 = 1.5;
/// Extra margin on top of the listen VAD's speech-threshold factor when seeding
/// the barge floor from the quiet-phase ambient noise: a barge must be *clearly*
/// louder than what merely counted as speech while nothing played.
const BARGE_AMBIENT_MARGIN: f32 = 1.5;
/// How many `(mic_rms, ref_rms, threshold)` triples to keep for the pre-barge
/// diagnostic dump.
const BARGE_HISTORY: usize = 10;
/// Cap on retained per-call diagnostic logs (this one plus the 4 most recent).
const CALL_LOG_KEEP: usize = 5;

/// A per-call diagnostic log at `config_dir()/logs/call-<unix_ts>.log`.
///
/// Plain text, one line per event, each stamped with milliseconds since the
/// call started. Entirely best-effort: if the directory or file can't be opened
/// every method degrades to a no-op, so diagnostics can never break a call. On
/// construction it prunes older logs, keeping only the [`CALL_LOG_KEEP`] most
/// recent (including the one about to be written).
struct CallLog {
    file: Mutex<Option<std::fs::File>>,
    start: Instant,
}

impl CallLog {
    /// Open (creating the `logs/` dir) and prune. Never fails — a filesystem
    /// error just yields a log whose methods are silent no-ops.
    fn new() -> Self {
        Self {
            file: Mutex::new(Self::open().ok()),
            start: Instant::now(),
        }
    }

    fn open() -> std::io::Result<std::fs::File> {
        let dir = AppConfig::config_dir().join("logs");
        std::fs::create_dir_all(&dir)?;
        Self::prune(&dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(format!("call-{ts}.log")))
    }

    /// Delete the oldest `call-*.log` files so that, once this call's file is
    /// added, at most [`CALL_LOG_KEEP`] remain.
    fn prune(dir: &std::path::Path) {
        let mut logs: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("call-") && n.ends_with(".log"))
            })
            .collect();
        // Oldest first (missing mtimes sort as the epoch → pruned first).
        logs.sort_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH)
        });
        let max_existing = CALL_LOG_KEEP.saturating_sub(1);
        while logs.len() > max_existing {
            let _ = std::fs::remove_file(logs.remove(0));
        }
    }

    /// Append one timestamped line. Silent on any error.
    fn line(&self, msg: &str) {
        if let Ok(mut g) = self.file.lock() {
            if let Some(f) = g.as_mut() {
                let ms = self.start.elapsed().as_millis();
                let _ = writeln!(f, "[{ms:>8}ms] {msg}");
                let _ = f.flush();
            }
        }
    }
}

/// Whether a hands-free call is currently running (checked by the ⌥S hotkey so
/// hold-to-talk stays disabled for the duration of a call).
pub fn is_call_active(state: &AppState) -> bool {
    state.call_active.load(Ordering::SeqCst)
}

/// Start a hands-free call. Idempotent: a second call while one is running is a
/// no-op. Refuses to start on top of an in-flight hold-to-talk turn.
#[tauri::command]
pub async fn call_start(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    if super::sts::turn_in_flight() || super::dictation::is_recording() {
        return Err("Busy — finish the current turn first.".into());
    }
    if state.call_active.swap(true, Ordering::SeqCst) {
        return Ok(()); // already in a call
    }
    let active = state.call_active.clone();
    let app2 = app.clone();
    // `call_started` is emitted from the loop once the audio path is known, so it
    // can carry which path engaged (the UI's AEC truth chip). The frontend shows
    // the call UI optimistically on the button press, so nothing waits on this.
    tauri::async_runtime::spawn(async move {
        run_call_loop(app2, active).await;
    });
    Ok(())
}

/// Hang up. Safe to call in any phase: it clears the active flag and stops both
/// capture and playback so the loop unblocks and tears down cleanly (the loop
/// emits the terminal `call_ended` event).
#[tauri::command]
pub async fn call_stop(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.call_active.store(false, Ordering::SeqCst);
    stop_capture(&state.audio_capture);
    if let Ok(g) = state.audio_playback.lock() {
        if let Some(p) = g.as_ref() {
            p.stop();
        }
    }
    Ok(())
}

/// The audio source the call loop drains chunks from.
///
/// With barge-in enabled we prefer *system echo cancellation* so the mic can
/// stay hot while the reply plays without the assistant's own voice bleeding in
/// and self-triggering the barge detector:
/// - macOS: the `fonos-voice-capture` helper (AVAudioEngine VPIO — AEC + noise
///   suppression + AGC), running FULL-DUPLEX: the reply's TTS also plays
///   through the helper's engine, because VPIO cancels only its own engine's
///   output — TTS played via rodio would leave the canceller with a silent
///   reference and cancel nothing. Self-owned; never touches the shared cpal
///   capture or the rodio sink.
/// - Linux: `module-echo-cancel` routed via [`crate::audio::linux_aec`], with a
///   plain cpal capture reading the (now-default) ec source.
/// - Fallback: the shared cpal capture, armed per phase via `start_recording`,
///   exactly as before — used with barge-in off, on other platforms, or when
///   AEC setup fails. The [`fonos_core::vad::BargeDetector`] envelope gating
///   stays on top of *all three* paths (belt and braces).
///
/// AEC variants keep the mic hot for the whole call; the fallback opens/closes
/// the mic per phase. `AEC ⟹ barge-in enabled`.
enum CallAudio {
    /// macOS voice-processing capture (system AEC via the Swift helper).
    #[cfg(target_os = "macos")]
    MacVoice(crate::audio::voice_capture::VoiceProcessedCapture),
    /// Linux: cpal capture on the ec source; the guard restores routing + unloads
    /// the module on drop.
    #[cfg(target_os = "linux")]
    LinuxEc {
        capture: AudioCapture,
        _guard: crate::audio::linux_aec::EchoCancelGuard,
    },
    /// Plain cpal via the shared `AppState` capture (current behavior).
    Fallback,
}

impl CallAudio {
    /// True when a system-AEC source is active (mic stays hot; no per-phase
    /// `start_recording`/`stop_capture`).
    fn is_aec(&self) -> bool {
        !matches!(self, CallAudio::Fallback)
    }

    /// Short tag for the live audio path — carried in the `call_started` event
    /// (the UI's "AEC" / "no AEC" truth chip) and the diagnostic log.
    fn path_label(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            CallAudio::MacVoice(_) => "aec",
            #[cfg(target_os = "linux")]
            CallAudio::LinuxEc { .. } => "ec",
            CallAudio::Fallback => "fallback",
        }
    }

    /// Drain the oldest `ms` of audio from whichever source is active. For the
    /// fallback, drains the shared cpal capture; for AEC, the self-owned source.
    fn take_chunk(
        &self,
        ms: u32,
        shared: &Arc<Mutex<Option<AudioCapture>>>,
    ) -> Option<Vec<i16>> {
        match self {
            #[cfg(target_os = "macos")]
            CallAudio::MacVoice(v) => v.take_chunk(ms),
            #[cfg(target_os = "linux")]
            CallAudio::LinuxEc { capture, .. } => capture.take_chunk(ms),
            CallAudio::Fallback => shared
                .lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|c| c.take_chunk(ms))),
        }
    }

    /// Discard everything currently buffered — used at AEC phase boundaries so a
    /// fresh listen/monitor starts clean (the mic never stopped).
    fn drain_stale(&self, shared: &Arc<Mutex<Option<AudioCapture>>>) {
        while self.take_chunk(VAD_CHUNK_MS, shared).is_some() {}
    }

    /// Whether the assistant's reply is audible right now. macOS voice mode
    /// plays TTS through the helper (rodio stays silent), so the signal comes
    /// from the helper's playback state; everywhere else it is the rodio queue.
    fn is_reply_playing(&self, playback: &Arc<Mutex<Option<AudioPlayback>>>) -> bool {
        #[cfg(target_os = "macos")]
        if let CallAudio::MacVoice(v) = self {
            return v.is_playing();
        }
        playback
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|p| !p.queue_empty()))
            .unwrap_or(false)
    }

    /// Cut the reply off NOW — barge interrupt / hangup. macOS voice mode
    /// flushes the helper's playback queue (the zero-length control frame);
    /// other paths stop the rodio sink as before.
    fn cut_reply_playback(&self, playback: &Arc<Mutex<Option<AudioPlayback>>>) {
        #[cfg(target_os = "macos")]
        if let CallAudio::MacVoice(v) = self {
            v.flush_playback();
            return;
        }
        if let Ok(g) = playback.lock() {
            if let Some(p) = g.as_ref() {
                p.stop();
            }
        }
    }
}

/// Run one conversation turn for the call loop, routing the spoken reply
/// through the call's audio source: macOS voice mode plays TTS via the helper
/// engine (so VPIO's echo canceller sees the true reference), everything else
/// keeps the default rodio playback + envelope reference.
async fn run_call_turn(
    app: &tauri::AppHandle,
    audio: &CallAudio,
    transcript: String,
    bridge: &crate::adapters::TurnEventBridge,
) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    if let CallAudio::MacVoice(v) = audio {
        let out = v.audio_out();
        return super::sts::execute_turn_with_audio(app, transcript, None, bridge, &out).await;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = audio; // only matched on macOS
    execute_turn(app, transcript, None, bridge).await
}

/// Pick the call's audio source. With barge-in enabled, try platform system echo
/// cancellation; on failure — or with barge-in off / other platforms — use the
/// plain cpal path. Emits a one-line note on which path engaged.
#[allow(unused_variables)]
fn setup_call_audio(
    barge_enabled: bool,
    device_name: &str,
    playback: &Arc<Mutex<Option<AudioPlayback>>>,
    log: &CallLog,
) -> CallAudio {
    if !barge_enabled {
        log.line("audio: barge-in disabled → Fallback (plain cpal, mic per-phase)");
        return CallAudio::Fallback;
    }

    #[cfg(target_os = "macos")]
    {
        let mut v = crate::audio::voice_capture::VoiceProcessedCapture::new(device_name);
        return match v.start() {
            Ok(()) => {
                // The helper prints READY once its engine is actually running;
                // a spawn that then failed VPIO/engine setup falls back here
                // instead of leaving the call deaf.
                if !v.wait_ready(Duration::from_secs(5)) {
                    let why = "helper spawned but never reported READY within 5s \
                               (VPIO/engine setup failed, or mic permission denied \
                               for the child process)";
                    eprintln!(
                        "fonos: call AEC unavailable ({why}); \
                         falling back to cpal + envelope gating"
                    );
                    log.line(&format!("audio: Fallback — AEC unavailable: {why}"));
                    return CallAudio::Fallback;
                }
                eprintln!("fonos: call AEC engaged — macOS voice-processing capture (VPIO)");
                eprintln!(
                    "fonos: call TTS — helper playback engaged (full-duplex; \
                     VPIO gets the true echo reference)"
                );
                log.line(
                    "audio: MacVoice engaged — VPIO full-duplex (mic hot whole call; \
                     TTS routed through the helper engine)",
                );
                CallAudio::MacVoice(v)
            }
            Err(e) => {
                eprintln!(
                    "fonos: call AEC unavailable ({e}); falling back to cpal + envelope gating"
                );
                log.line(&format!("audio: Fallback — helper start failed: {e}"));
                CallAudio::Fallback
            }
        };
    }

    #[cfg(target_os = "linux")]
    {
        match crate::audio::linux_aec::setup() {
            Ok(guard) => {
                // The cached playback instance (if any) is bound to the *old*
                // default sink; drop it so the next turn reopens on the ec sink.
                if let Ok(mut g) = playback.lock() {
                    *g = None;
                }
                // Capture from the ec source: prefer a strict match on its pulse
                // name, else the default input (now the ec source after
                // set-default-source).
                let opened = AudioCapture::with_device(crate::audio::linux_aec::SOURCE_NAME)
                    .or_else(|_| AudioCapture::new());
                let mut capture = match opened {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("fonos: call AEC — mic open failed ({e}); falling back");
                        log.line(&format!("audio: Fallback — ec mic open failed: {e}"));
                        return CallAudio::Fallback; // guard drops → routing restored
                    }
                };
                if let Err(e) = capture.start() {
                    eprintln!("fonos: call AEC — mic start failed ({e}); falling back");
                    log.line(&format!("audio: Fallback — ec mic start failed: {e}"));
                    return CallAudio::Fallback; // guard drops → routing restored
                }
                eprintln!("fonos: call AEC engaged — Linux module-echo-cancel");
                log.line("audio: LinuxEc engaged — module-echo-cancel source");
                return CallAudio::LinuxEc { capture, _guard: guard };
            }
            Err(e) => {
                eprintln!(
                    "fonos: call AEC unavailable ({e}); falling back to cpal + envelope gating"
                );
                log.line(&format!("audio: Fallback — module-echo-cancel setup failed: {e}"));
                return CallAudio::Fallback;
            }
        }
    }

    #[allow(unreachable_code)]
    {
        log.line("audio: Fallback — no system AEC on this platform");
        CallAudio::Fallback
    }
}

/// Why a call ended, carried in the terminal `call_ended` event.
enum Outcome {
    /// The user hung up (or capture failed).
    Cancelled,
    /// No speech for `timeout_ms` — auto hang-up.
    Timeout,
    /// An utterance was captured; transcribe + run a turn.
    Utterance,
}

/// The call loop: runs until the active flag clears or the VAD times out.
async fn run_call_loop(app: tauri::AppHandle, active: Arc<AtomicBool>) {
    // Snapshot the tuning + shared handles once.
    let (sensitivity, silence_ms, barge_enabled, device_name) = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| {
            (
                c.call_vad_sensitivity,
                c.call_vad_silence_ms,
                c.call_barge_in,
                c.audio_input_device.clone(),
            )
        })
        .unwrap_or((0.5, 800, true, "auto".to_string()));
    let mut vad_cfg = VadConfig {
        sensitivity,
        silence_hang_ms: silence_ms.clamp(500, 2000),
        ..Default::default()
    };
    let capture = app.state::<AppState>().audio_capture.clone();
    let playback = app.state::<AppState>().audio_playback.clone();

    // Per-call diagnostic log (best-effort; see [`CallLog`]).
    let log = CallLog::new();
    log.line(&format!(
        "call: start — sensitivity={sensitivity:.2} silence_hang_ms={} barge_in={barge_enabled} \
         device={device_name:?}",
        vad_cfg.silence_hang_ms
    ));

    // Choose the audio source for the whole call. With barge-in on, this engages
    // system echo cancellation (macOS VPIO / Linux module-echo-cancel) and keeps
    // the mic hot for the entire session; on failure or with barge-in off it is
    // the plain cpal path armed per phase. The BargeDetector envelope gating runs
    // on top of all three (belt and braces).
    let audio = setup_call_audio(barge_enabled, &device_name, &playback, &log);
    // On a processed-audio path the listen VAD's raw-cpal absolute-minimum clamp
    // would leave the call deaf (near-zero silence floor, AGC-levelled speech);
    // replace it with a small fixed floor so the scale-free relative bar drives
    // detection. Fallback keeps the raw-cpal ramp.
    if audio.is_aec() {
        vad_cfg.abs_min_threshold = Some(LISTEN_ABS_MIN_AEC);
    }
    log.line(&format!(
        "listen: VAD abs_min_threshold={:?} (path={})",
        vad_cfg.abs_min_threshold,
        audio.path_label()
    ));
    // Tell the page which audio path is live so it can render the AEC truth chip.
    emit_call_started(&app, audio.path_label());
    // The barge floor's ambient-seed multiplier: the same "clearly louder than
    // the quiet room" notion the listen VAD uses, plus an extra margin.
    let ambient_k = speech_threshold_factor(sensitivity) * BARGE_AMBIENT_MARGIN;
    // No pill for call-mode turns — everything renders in the Conversation page.
    let bridge = crate::adapters::TurnEventBridge::new(app.clone(), false);

    // Reason reported when the loop exits.
    let mut ended = "hangup";

    // Barge carry-over: when the user interrupts the reply, the monitor's
    // buffered samples (pre-roll + interrupting words) are stashed here and
    // seeded into the next listen so nothing they said is lost.
    let mut seed: Option<Vec<i16>> = None;

    'session: loop {
        if !active.load(Ordering::SeqCst) {
            break;
        }

        // ── LISTEN: arm the mic and watch the VAD ──
        emit_call(&app, "call_listening", "");
        if audio.is_aec() {
            // AEC source stays hot for the whole call; just drop anything it
            // buffered since the last phase so the utterance starts clean.
            audio.drain_stale(&capture);
        } else {
            // Fallback: arm the shared cpal mic exactly as before.
            let st = app.state::<AppState>();
            if let Err(e) = super::dictation::start_recording(app.clone(), st, Some(true)).await {
                emit_call(&app, "error", &e);
                break 'session;
            }
        }

        let mut vad = VadSession::new(vad_cfg.clone());
        let mut buf: Vec<i16> = Vec::new();
        // Seed from a barge: prime the VAD (into its Speech phase) and the
        // utterance buffer with the interrupting words, so a short interruption
        // that ended just before the barge fired isn't stranded — the trailing
        // silence still resolves it into a full turn.
        if let Some(carry) = seed.take() {
            for chunk in carry.chunks(CHUNK_SAMPLES) {
                let _ = vad.push(chunk);
                buf.extend_from_slice(chunk);
            }
        }
        // Per-second listen-phase level profile — the mic energy the VAD sees,
        // plus its live noise floor and effective threshold, so a "call can't
        // hear me" symptom is directly readable from the log (mirrors the SPEAK
        // monitor's per-second line).
        let mut sec_mic: Vec<f32> = Vec::new();
        let mut sec_ms: u32 = 0;
        let outcome = loop {
            if !active.load(Ordering::SeqCst) {
                break Outcome::Cancelled;
            }
            // Drain one chunk WITHOUT losing it: we accumulate every chunk into
            // `buf` and feed a copy to the VAD, so the same samples that decide
            // "utterance over" are the ones we transcribe.
            let chunk = audio.take_chunk(VAD_CHUNK_MS, &capture);
            match chunk {
                Some(samples) => {
                    let ev = vad.push(&samples);
                    buf.extend_from_slice(&samples);
                    sec_mic.push(rms(&samples));
                    sec_ms += (samples.len() as u32) / 16; // 16 samples/ms @ 16 kHz
                    if sec_ms >= 1_000 {
                        log.line(&format!(
                            "listen: {}s — mic[min/avg/max]={:.0}/{:.0}/{:.0} \
                             noise_floor={:.0} threshold={:.0}",
                            sec_ms / 1_000,
                            min(&sec_mic),
                            avg(&sec_mic),
                            max(&sec_mic),
                            vad.noise_floor(),
                            vad.threshold()
                        ));
                        sec_mic.clear();
                        sec_ms = 0;
                    }
                    match ev {
                        VadEvent::UtteranceEnd => break Outcome::Utterance,
                        VadEvent::Timeout => break Outcome::Timeout,
                        _ => {}
                    }
                }
                None => tokio::time::sleep(Duration::from_millis(POLL_MS)).await,
            }
        };

        // Mic OFF before the reply plays (v1 echo avoidance) — fallback path
        // only. With AEC the mic stays hot; echo cancellation removes the bleed
        // and the barge monitor drains from the same live source.
        if !audio.is_aec() {
            stop_capture(&capture);
        }

        // The listen VAD's learned ambient noise floor — the user's real room
        // level, measured while nothing played — seeds the barge floor below.
        let ambient_floor = vad.noise_floor();

        match outcome {
            Outcome::Cancelled => {
                ended = "hangup";
                break 'session;
            }
            Outcome::Timeout => {
                log.line("listen: VAD timeout — no speech; hanging up");
                ended = "timeout";
                break 'session;
            }
            Outcome::Utterance => {
                log.line(&format!(
                    "listen: utterance end — {} samples (~{} ms), learned noise_floor={ambient_floor:.0}",
                    buf.len(),
                    buf.len() / 16
                ));
            }
        }

        // ── TRANSCRIBE the accumulated utterance (shared STT path) ──
        let stt = {
            let st = app.state::<AppState>();
            transcribe_samples(&app, st.inner(), buf, Some("sts-page".to_string())).await
        };
        let transcript = match stt {
            Ok(r) => r.text.trim().to_string(),
            Err(e) => {
                // STT failed — surface it, but keep the call alive and re-arm.
                log.line(&format!("stt: error — {e}"));
                emit_call(&app, "error", &e);
                continue 'session;
            }
        };
        if transcript.is_empty() {
            // VAD produced an empty / too-short utterance: no "No speech
            // detected" bubble in call mode — just listen again.
            log.line("stt: empty transcript — re-listening");
            continue 'session;
        }
        log.line(&format!("stt: transcript ({} chars)", transcript.chars().count()));

        // ── THINK + SPEAK ──
        // Errors surface through the bridge; keep the call alive on failure so
        // a transient hiccup doesn't hang up. Then loop back to LISTEN.
        if barge_enabled {
            // Run the reply and a barge monitor concurrently. The turn future
            // only synthesizes + plays (it never touches capture); the monitor
            // owns capture. They can't fight over the lock.
            let turn_fut = run_call_turn(&app, &audio, transcript, &bridge);
            let barge_fut =
                barge_monitor(&app, &audio, &capture, &playback, &active, &log, ambient_floor, ambient_k);
            tokio::pin!(turn_fut);
            tokio::pin!(barge_fut);
            tokio::select! {
                reply = &mut turn_fut => {
                    // The reply finished on its own — tear down the monitor
                    // capture and discard whatever it buffered (any speech there
                    // that didn't trip the barge was sub-threshold bleed). AEC
                    // keeps the mic hot; the next listen drains its stale tail.
                    match reply {
                        Ok(text) => log.line(&format!(
                            "speak: reply played to completion ({} chars)",
                            text.chars().count()
                        )),
                        Err(e) => log.line(&format!("speak: turn error — {e}")),
                    }
                    if !audio.is_aec() {
                        stop_capture(&capture);
                    }
                }
                barged = &mut barge_fut => {
                    match barged {
                        Some(carry) => {
                            // Cut the reply off: stop playback now (helper
                            // flush in macOS voice mode, rodio stop elsewhere),
                            // and let the turn future drop (below) — that
                            // cancels any in-flight synthesis HTTP. NOTE:
                            // because we abort mid-turn, run_turn's end-of-turn
                            // session-history push never runs, so the truncated
                            // reply is not remembered — intended, the user cut
                            // it off.
                            audio.cut_reply_playback(&playback);
                            log.line(&format!(
                                "speak: BARGE confirmed — cutting reply, carrying {} samples \
                                 (~{} ms) into next listen",
                                carry.len(),
                                carry.len() / 16
                            ));
                            emit_call(&app, "barge", "");
                            // Mic stays live: carry the interrupting words into
                            // the next listen, which starts immediately.
                            seed = Some(carry);
                        }
                        None => {
                            // Cancelled / capture failed during the reply.
                            log.line("speak: monitor ended without barge (cancelled/capture-fail)");
                            if !audio.is_aec() {
                                stop_capture(&capture);
                            }
                        }
                    }
                }
            }
            // Both futures are dropped here; dropping `turn_fut` releases the
            // sts_session lock and cancels any in-flight synthesis cleanly.
        } else {
            // Barge-in off: exact v1 behavior — mic closed, reply plays through.
            let _ = execute_turn(&app, transcript, None, &bridge).await;
        }

        if !active.load(Ordering::SeqCst) {
            ended = "hangup";
            break 'session;
        }
    }

    // ── CLEANUP ──
    // Fallback path: stop the shared cpal capture (no-op in AEC mode, where it
    // was never armed). Then stop playback — every hangup path (`call_stop`
    // included; it just flips `active`) funnels through here, so this is also
    // where macOS voice mode flushes the helper's playback queue — and tear
    // down the AEC source.
    stop_capture(&capture);
    audio.cut_reply_playback(&playback);
    if let Ok(g) = playback.lock() {
        if let Some(p) = g.as_ref() {
            p.stop();
        }
    }
    // Dropping `audio` runs the AEC teardown via RAII: macOS kills the helper;
    // Linux restores the prior default sink/source and unloads module-echo-cancel.
    // This is the single teardown point every loop-exit path funnels through
    // (hangup / timeout / error / `call_stop`, which flips `active` so the loop
    // unwinds here).
    let was_aec = audio.is_aec();
    drop(audio);
    // Linux: the cached playback instance was bound to the ec sink; drop it so
    // the next (non-call) TTS reopens on the now-restored default output.
    #[cfg(target_os = "linux")]
    if was_aec {
        if let Ok(mut g) = playback.lock() {
            *g = None;
        }
    }
    let _ = was_aec; // only read on Linux
    active.store(false, Ordering::SeqCst);
    log.line(&format!("call: end — reason={ended}"));
    emit_call(&app, "call_ended", ended);
}

/// Watch the mic for a genuine interruption while the reply plays.
///
/// Returns `Some(carry)` — a pre-roll + trigger buffer to seed the next listen
/// — when a barge is *confirmed*, or `None` if the call was cancelled (or
/// capture couldn't start) before any confirmed barge.
///
/// Detection is two-stage. [`BargeDetector`] first fires a SOFT trigger when
/// the mic sustains energy above the bleed; because AEC residual echo can
/// briefly do the same (~2–3 false cuts per 20 turns), the monitor then opens a
/// ~[`BARGE_VERIFY_WINDOW_MS`] verify window instead of cutting immediately.
/// Playback keeps going throughout — overlap is natural — and the trigger is
/// CONFIRMED only if mic energy stays above a *raised* bar
/// ([`BargeDetector::threshold_for`], `abs_mult` = [`BARGE_VERIFY_BAR_MULT`])
/// for [`BARGE_VERIFY_VOICED_MS`] cumulative; a real user through AEC runs well
/// above the residual, so marginal bursts are REFUTED (run reset, reply
/// uninterrupted, monitoring continues).
///
/// Detection is reference-gated: because we own the TTS PCM, each mic chunk is
/// compared against [`AudioPlayback::reference_rms`] — the loudness the speaker
/// is emitting *right now*. The assistant's own voice bleeds into the mic in
/// proportion to that reference, so it never trips the detector however loud it
/// swells; only mic energy with no matching reference rise (the user talking
/// over it) counts. Warmup is aligned with playback: the monitor first drains
/// and discards everything the mic hears until the reply is actually audible,
/// so [`BargeDetector`] learns the speaker→mic coupling (not the pre-playback
/// quiet) before it starts listening for the user talking over it.
#[allow(clippy::too_many_arguments)]
async fn barge_monitor(
    app: &tauri::AppHandle,
    audio: &CallAudio,
    capture: &Arc<Mutex<Option<AudioCapture>>>,
    playback: &Arc<Mutex<Option<AudioPlayback>>>,
    active: &Arc<AtomicBool>,
    log: &CallLog,
    ambient_floor: f32,
    ambient_k: f32,
) -> Option<Vec<i16>> {
    // Arm the mic (skip the float pill — call mode never shows it). With AEC the
    // mic is already hot for the whole call, so we skip arming and just drain
    // from the same live source.
    if !audio.is_aec() {
        let st = app.state::<AppState>();
        if super::dictation::start_recording(app.clone(), st, Some(true))
            .await
            .is_err()
        {
            return None;
        }
    }

    // Phase 1 — wait for playback to begin, discarding anything captured so the
    // VAD warmup starts aligned with the bleed rather than the prior quiet.
    // (With AEC the "bleed" is near-silence post-cancellation, so the detector
    // then flags essentially only the user talking — belt and braces.)
    loop {
        if !active.load(Ordering::SeqCst) {
            return None;
        }
        // Drain the ring buffer so no stale pre-playback audio survives.
        audio.drain_stale(capture);
        if audio.is_reply_playing(playback) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }
    log.line(&format!(
        "speak: playback audible — barge detector armed \
         (ambient_floor={ambient_floor:.0} k={ambient_k:.2} ambient_seed={:.0} \
         warmup={BARGE_WARMUP_MS}ms sustained={BARGE_SUSTAINED_MS}ms)",
        ambient_floor * ambient_k
    ));

    // Phase 2 — warmup + detect. Buffer everything; the detector learns the
    // playback→mic coupling during warmup, then confirms a barge only when the
    // mic sustains energy clearly above the bleed expected for the reference
    // playing at that instant. Its absolute floor is seeded with the listen
    // phase's ambient noise so post-AEC residual can't self-trigger.
    // Processed-audio warmup sees near-silence, so its peak-bleed term collapses
    // to this hard minimum: keep the conservative 80 for raw cpal, but a low
    // bound for AEC paths (the ambient-seeded floor governs there instead).
    let barge_abs_min = if audio.is_aec() {
        BARGE_ABS_MIN_AEC
    } else {
        BARGE_ABS_MIN_FALLBACK
    };
    let mut detector = BargeDetector::new(
        BARGE_WARMUP_MS,
        BARGE_SUSTAINED_MS,
        ambient_floor,
        ambient_k,
        barge_abs_min,
    );
    let mut buf: Vec<i16> = Vec::new();
    // Diagnostics: last N (mic, ref, threshold) triples for the pre-barge dump,
    // and a rolling per-second min/avg/max of mic/ref energy.
    let mut recent: VecDeque<(f32, f32, f32)> = VecDeque::with_capacity(BARGE_HISTORY);
    let mut sec_mic: Vec<f32> = Vec::new();
    let mut sec_ref: Vec<f32> = Vec::new();
    let mut sec_ms: u32 = 0;
    loop {
        if !active.load(Ordering::SeqCst) {
            return None;
        }
        let chunk = audio.take_chunk(VAD_CHUNK_MS, capture);
        match chunk {
            Some(samples) => {
                // Mic energy for this chunk, and the live playback reference it
                // is gated against (0.0 when the queue has drained). macOS
                // voice mode plays TTS through the helper, so the rodio
                // reference reads 0 there — intended: with true AEC the
                // residual bleed is tiny and the detector's absolute floor
                // (its ref_rms == 0 fallback) governs.
                let mic_rms = rms(&samples);
                let ref_rms = playback
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().map(|p| p.reference_rms()))
                    .unwrap_or(0.0);
                let chunk_ms = (samples.len() as u32) / 16; // 16 samples/ms @ 16 kHz
                buf.extend_from_slice(&samples);

                let was_warming = detector.is_warming_up();
                let barged = detector.push(mic_rms, ref_rms, chunk_ms);
                // Log the warmup summary exactly once, when warmup ends.
                if was_warming && !detector.is_warming_up() {
                    log.line(&format!(
                        "speak: warmup done — coupling={:.3} peak_bleed={:.0} abs_floor={:.0}",
                        detector.coupling(),
                        detector.peak_bleed(),
                        detector.abs_floor()
                    ));
                }
                // Post-warmup chunks feed the diagnostics.
                if !was_warming {
                    if recent.len() == BARGE_HISTORY {
                        recent.pop_front();
                    }
                    recent.push_back((mic_rms, ref_rms, detector.last_threshold()));
                    sec_mic.push(mic_rms);
                    sec_ref.push(ref_rms);
                    sec_ms += chunk_ms;
                    if sec_ms >= 1_000 {
                        log.line(&format!(
                            "speak: {}s — mic[min/avg/max]={:.0}/{:.0}/{:.0} \
                             ref[min/avg/max]={:.0}/{:.0}/{:.0}",
                            sec_ms / 1_000,
                            min(&sec_mic),
                            avg(&sec_mic),
                            max(&sec_mic),
                            min(&sec_ref),
                            avg(&sec_ref),
                            max(&sec_ref)
                        ));
                        sec_mic.clear();
                        sec_ref.clear();
                        sec_ms = 0;
                    }
                }

                if barged {
                    // ── SOFT barge: don't cut yet. The detector fired, but AEC
                    // residual echo can sustain past the trigger ~2–3× per 20
                    // turns and self-cut the reply. Mark the point and open a
                    // verify window: playback keeps going (overlap is natural),
                    // and only mic energy that stays *clearly* above the trigger
                    // bar for BARGE_VERIFY_VOICED_MS confirms a real user. ──
                    log.line(&format!(
                        "barge: soft — verifying (playback continues); trigger run={}ms \
                         abs_floor={:.0}; last {} chunks (mic,ref,thr):",
                        detector.run_ms(),
                        detector.abs_floor(),
                        recent.len()
                    ));
                    for (m, r, t) in &recent {
                        log.line(&format!("    mic={m:.0} ref={r:.0} thr={t:.0}"));
                    }

                    let verify_start_len = buf.len();
                    let mut verify_ms: u32 = 0;
                    let mut voiced_ms: u32 = 0;
                    let mut vmic: Vec<f32> = Vec::new();
                    let mut last_bar = 0.0f32;
                    // Verify window: keep draining the (still-live) mic into the
                    // carry buffer, counting time above the raised bar. Ends
                    // early the moment enough voiced time accrues.
                    while verify_ms < BARGE_VERIFY_WINDOW_MS && voiced_ms < BARGE_VERIFY_VOICED_MS {
                        if !active.load(Ordering::SeqCst) {
                            return None;
                        }
                        match audio.take_chunk(VAD_CHUNK_MS, capture) {
                            Some(samples) => {
                                let mic_rms = rms(&samples);
                                // Live playback loudness for the coupling bar
                                // (0.0 on the macOS helper path — abs_floor
                                // governs there).
                                let ref_rms = playback
                                    .lock()
                                    .ok()
                                    .and_then(|g| g.as_ref().map(|p| p.reference_rms()))
                                    .unwrap_or(0.0);
                                let bar = detector.threshold_for(ref_rms, BARGE_VERIFY_BAR_MULT);
                                last_bar = bar;
                                let chunk_ms = (samples.len() as u32) / 16;
                                buf.extend_from_slice(&samples);
                                vmic.push(mic_rms);
                                verify_ms += chunk_ms;
                                if mic_rms > bar {
                                    voiced_ms += chunk_ms;
                                }
                            }
                            None => tokio::time::sleep(Duration::from_millis(POLL_MS)).await,
                        }
                    }

                    if voiced_ms >= BARGE_VERIFY_VOICED_MS {
                        // ── CONFIRMED: a real interruption. Hand back the carry
                        // (pre-roll + soft-window audio); the caller does exactly
                        // today's hard cut — flush/stop playback, cancel the
                        // turn, emit {kind:"barge"}, seed the next listen. ──
                        log.line(&format!(
                            "barge: confirmed ({voiced_ms} ms voiced above raised bar={last_bar:.0}; \
                             verify mic[min/avg/max]={:.0}/{:.0}/{:.0})",
                            min(&vmic),
                            avg(&vmic),
                            max(&vmic)
                        ));
                        let start = buf.len().saturating_sub(BARGE_PREROLL_SAMPLES);
                        return Some(buf[start..].to_vec());
                    }

                    // ── REFUTED: nothing audible happened — the sustained energy
                    // was echo residual, not the user. Leave the reply playing,
                    // drop the soft-window audio from the carry, and require a
                    // fresh full sustained run before the next soft attempt. ──
                    log.line(&format!(
                        "barge: refuted — residual, playback uninterrupted \
                         ({voiced_ms} ms < {BARGE_VERIFY_VOICED_MS} above bar={last_bar:.0}; \
                         verify mic[min/avg/max]={:.0}/{:.0}/{:.0})",
                        min(&vmic),
                        avg(&vmic),
                        max(&vmic)
                    ));
                    buf.truncate(verify_start_len);
                    detector.reset_run();
                    // Fall through: keep monitoring from the next chunk.
                }
            }
            None => tokio::time::sleep(Duration::from_millis(POLL_MS)).await,
        }
    }
}

/// Min of an RMS slice (`0.0` if empty).
fn min(xs: &[f32]) -> f32 {
    xs.iter().copied().fold(f32::INFINITY, f32::min).min(max(xs))
}
/// Max of an RMS slice (`0.0` if empty).
fn max(xs: &[f32]) -> f32 {
    xs.iter().copied().fold(0.0, f32::max)
}
/// Mean of an RMS slice (`0.0` if empty).
fn avg(xs: &[f32]) -> f32 {
    if xs.is_empty() {
        0.0
    } else {
        xs.iter().sum::<f32>() / xs.len() as f32
    }
}

/// Stop the live capture stream and clear the recording flag. The call loop
/// owns capture directly, so it can't go through `stop_recording` (which would
/// drain + transcribe the leftover tail).
fn stop_capture(capture: &Arc<Mutex<Option<AudioCapture>>>) {
    if let Ok(mut g) = capture.lock() {
        if let Some(c) = g.as_mut() {
            c.stop();
        }
    }
    super::dictation::clear_recording_flag();
}

/// Mirror a call-lifecycle event onto the `sts:event` channel the Conversation
/// page listens on (same shape as [`crate::adapters::TurnEventBridge`]).
fn emit_call(app: &tauri::AppHandle, kind: &str, text: &str) {
    let _ = app.emit("sts:event", serde_json::json!({ "kind": kind, "text": text }));
}

/// The `call_started` event, carrying which audio path engaged (`"aec"` / `"ec"`
/// / `"fallback"`) so the Conversation page can render its AEC truth chip.
fn emit_call_started(app: &tauri::AppHandle, audio: &str) {
    let _ = app.emit(
        "sts:event",
        serde_json::json!({ "kind": "call_started", "text": "", "audio": audio }),
    );
}
