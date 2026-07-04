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
    let (sensitivity, silence_ms, barge_enabled) = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| (c.call_vad_sensitivity, c.call_vad_silence_ms, c.call_barge_in))
        .unwrap_or((0.5, 800, true));
    let vad_cfg = VadConfig {
        sensitivity,
        silence_hang_ms: silence_ms.clamp(500, 2000),
        ..Default::default()
    };
    let capture = app.state::<AppState>().audio_capture.clone();
    let playback = app.state::<AppState>().audio_playback.clone();
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
        {
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
            let chunk = capture
                .lock()
                .ok()
                .and_then(|g| g.as_ref().and_then(|c| c.take_chunk(VAD_CHUNK_MS)));
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

        // Mic OFF before the reply plays (v1 echo avoidance).
        stop_capture(&capture);

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
            let barge_fut = barge_monitor(&app, &capture, &playback, &active);
            tokio::pin!(turn_fut);
            tokio::pin!(barge_fut);
            tokio::select! {
                _ = &mut turn_fut => {
                    // The reply finished on its own — tear down the monitor
                    // capture and discard whatever it buffered (any speech there
                    // that didn't trip the barge was sub-threshold bleed).
                    stop_capture(&capture);
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
                            stop_capture(&capture);
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
    stop_capture(&capture);
    if let Ok(g) = playback.lock() {
        if let Some(p) = g.as_ref() {
            p.stop();
        }
    }
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
    capture: &Arc<Mutex<Option<AudioCapture>>>,
    playback: &Arc<Mutex<Option<AudioPlayback>>>,
    active: &Arc<AtomicBool>,
) -> Option<Vec<i16>> {
    // Arm the mic (skip the float pill — call mode never shows it).
    {
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
    loop {
        if !active.load(Ordering::SeqCst) {
            return None;
        }
        // Drain the ring buffer so no stale pre-playback audio survives.
        while capture
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|c| c.take_chunk(VAD_CHUNK_MS)))
            .is_some()
        {}
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
        let chunk = capture
            .lock()
            .ok()
            .and_then(|g| g.as_ref().and_then(|c| c.take_chunk(VAD_CHUNK_MS)));
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
