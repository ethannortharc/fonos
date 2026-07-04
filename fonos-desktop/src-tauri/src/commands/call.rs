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
//! above the bleed expected for whatever is playing at that instant. On a barge
//! it stops playback, cancels the in-flight turn, and carries the interrupting
//! words straight into the next listen. With barge-in off, the mic stays closed
//! during playback (the original behavior).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{Emitter, Manager};

use fonos_core::vad::{rms, BargeDetector, VadConfig, VadEvent, VadSession};

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
/// Pre-roll carried into the next listen so the interrupting words aren't lost
/// (16 000 samples ≈ 1 s at 16 kHz).
const BARGE_PREROLL_SAMPLES: usize = 16_000;

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
    emit_call(&app, "call_started", "");
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
///   suppression + AGC). Self-owned; never touches the shared cpal capture.
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
}

/// Pick the call's audio source. With barge-in enabled, try platform system echo
/// cancellation; on failure — or with barge-in off / other platforms — use the
/// plain cpal path. Emits a one-line note on which path engaged.
#[allow(unused_variables)]
fn setup_call_audio(
    barge_enabled: bool,
    device_name: &str,
    playback: &Arc<Mutex<Option<AudioPlayback>>>,
) -> CallAudio {
    if !barge_enabled {
        return CallAudio::Fallback;
    }

    #[cfg(target_os = "macos")]
    {
        let mut v = crate::audio::voice_capture::VoiceProcessedCapture::new(device_name);
        return match v.start() {
            Ok(()) => {
                eprintln!("fonos: call AEC engaged — macOS voice-processing capture (VPIO)");
                CallAudio::MacVoice(v)
            }
            Err(e) => {
                eprintln!(
                    "fonos: call AEC unavailable ({e}); falling back to cpal + envelope gating"
                );
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
                        return CallAudio::Fallback; // guard drops → routing restored
                    }
                };
                if let Err(e) = capture.start() {
                    eprintln!("fonos: call AEC — mic start failed ({e}); falling back");
                    return CallAudio::Fallback; // guard drops → routing restored
                }
                eprintln!("fonos: call AEC engaged — Linux module-echo-cancel");
                return CallAudio::LinuxEc { capture, _guard: guard };
            }
            Err(e) => {
                eprintln!(
                    "fonos: call AEC unavailable ({e}); falling back to cpal + envelope gating"
                );
                return CallAudio::Fallback;
            }
        }
    }

    #[allow(unreachable_code)]
    {
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
    let vad_cfg = VadConfig {
        sensitivity,
        silence_hang_ms: silence_ms.clamp(500, 2000),
        ..Default::default()
    };
    let capture = app.state::<AppState>().audio_capture.clone();
    let playback = app.state::<AppState>().audio_playback.clone();

    // Choose the audio source for the whole call. With barge-in on, this engages
    // system echo cancellation (macOS VPIO / Linux module-echo-cancel) and keeps
    // the mic hot for the entire session; on failure or with barge-in off it is
    // the plain cpal path armed per phase. The BargeDetector envelope gating runs
    // on top of all three (belt and braces).
    let audio = setup_call_audio(barge_enabled, &device_name, &playback);
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

        match outcome {
            Outcome::Cancelled => {
                ended = "hangup";
                break 'session;
            }
            Outcome::Timeout => {
                ended = "timeout";
                break 'session;
            }
            Outcome::Utterance => {}
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
                emit_call(&app, "error", &e);
                continue 'session;
            }
        };
        if transcript.is_empty() {
            // VAD produced an empty / too-short utterance: no "No speech
            // detected" bubble in call mode — just listen again.
            continue 'session;
        }

        // ── THINK + SPEAK ──
        // Errors surface through the bridge; keep the call alive on failure so
        // a transient hiccup doesn't hang up. Then loop back to LISTEN.
        if barge_enabled {
            // Run the reply and a barge monitor concurrently. The turn future
            // only synthesizes + plays (it never touches capture); the monitor
            // owns capture. They can't fight over the lock.
            let turn_fut = execute_turn(&app, transcript, None, &bridge);
            let barge_fut = barge_monitor(&app, &audio, &capture, &playback, &active);
            tokio::pin!(turn_fut);
            tokio::pin!(barge_fut);
            tokio::select! {
                _ = &mut turn_fut => {
                    // The reply finished on its own — tear down the monitor
                    // capture and discard whatever it buffered (any speech there
                    // that didn't trip the barge was sub-threshold bleed). AEC
                    // keeps the mic hot; the next listen drains its stale tail.
                    if !audio.is_aec() {
                        stop_capture(&capture);
                    }
                }
                barged = &mut barge_fut => {
                    match barged {
                        Some(carry) => {
                            // Cut the reply off: stop playback now, and let the
                            // turn future drop (below) — that cancels any
                            // in-flight synthesis HTTP. NOTE: because we abort
                            // mid-turn, run_turn's end-of-turn session-history
                            // push never runs, so the truncated reply is not
                            // remembered — intended, the user cut it off.
                            if let Ok(g) = playback.lock() {
                                if let Some(p) = g.as_ref() { p.stop(); }
                            }
                            emit_call(&app, "barge", "");
                            // Mic stays live: carry the interrupting words into
                            // the next listen, which starts immediately.
                            seed = Some(carry);
                        }
                        None => {
                            // Cancelled / capture failed during the reply.
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
    // was never armed). Then stop playback and tear down the AEC source.
    stop_capture(&capture);
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
    emit_call(&app, "call_ended", ended);
}

/// Watch the mic for a genuine interruption while the reply plays.
///
/// Returns `Some(carry)` — a pre-roll + trigger buffer to seed the next listen
/// — when the mic sustains energy clearly above the live playback bleed, or
/// `None` if the call was cancelled (or capture couldn't start) before any
/// barge.
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
async fn barge_monitor(
    app: &tauri::AppHandle,
    audio: &CallAudio,
    capture: &Arc<Mutex<Option<AudioCapture>>>,
    playback: &Arc<Mutex<Option<AudioPlayback>>>,
    active: &Arc<AtomicBool>,
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
        let playing = playback
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|p| !p.queue_empty()))
            .unwrap_or(false);
        if playing {
            break;
        }
        tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
    }

    // Phase 2 — warmup + detect. Buffer everything; the detector learns the
    // playback→mic coupling during warmup, then confirms a barge only when the
    // mic sustains energy clearly above the bleed expected for the reference
    // playing at that instant.
    let mut detector = BargeDetector::new(BARGE_WARMUP_MS, BARGE_SUSTAINED_MS);
    let mut buf: Vec<i16> = Vec::new();
    loop {
        if !active.load(Ordering::SeqCst) {
            return None;
        }
        let chunk = audio.take_chunk(VAD_CHUNK_MS, capture);
        match chunk {
            Some(samples) => {
                // Mic energy for this chunk, and the live playback reference it
                // is gated against (0.0 when the queue has drained).
                let mic_rms = rms(&samples);
                let ref_rms = playback
                    .lock()
                    .ok()
                    .and_then(|g| g.as_ref().map(|p| p.reference_rms()))
                    .unwrap_or(0.0);
                let chunk_ms = (samples.len() as u32) / 16; // 16 samples/ms @ 16 kHz
                buf.extend_from_slice(&samples);
                if detector.push(mic_rms, ref_rms, chunk_ms) {
                    // Keep a short pre-roll (the words that led up to the
                    // trigger) plus everything after it.
                    let start = buf.len().saturating_sub(BARGE_PREROLL_SAMPLES);
                    return Some(buf[start..].to_vec());
                }
            }
            None => tokio::time::sleep(Duration::from_millis(POLL_MS)).await,
        }
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
