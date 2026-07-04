//! Hands-free "call mode" for the STS conversation (issue #24).
//!
//! Where hold-to-talk is one turn per press, call mode runs a continuous loop:
//! listen → detect end-of-utterance (energy VAD, [`fonos_core::vad`]) →
//! transcribe → LLM → spoken reply → listen again, until the user hangs up.
//!
//! The loop is a single background task. It drives capture directly — draining
//! chunks for the VAD and, on [`VadEvent::UtteranceEnd`], transcribing the
//! accumulated buffer through the shared [`transcribe_samples`] path, then
//! running the same [`execute_turn`] pipeline as hold-to-talk. Because the
//! phases are sequential, the mic is always stopped before the reply plays —
//! that is the v1 echo-avoidance strategy (barge-in is v2, out of scope).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{Emitter, Manager};

use fonos_core::vad::{VadConfig, VadEvent, VadSession};

use crate::audio::capture::AudioCapture;
use super::AppState;
use super::dictation::transcribe_samples;
use super::sts::execute_turn;

/// Chunk size (ms) drained from the ring buffer and fed to the VAD each poll.
const VAD_CHUNK_MS: u32 = 100;
/// How long to nap when the ring buffer doesn't yet hold a full chunk.
const POLL_MS: u64 = 30;

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
    let sensitivity = app
        .state::<AppState>()
        .config
        .lock()
        .map(|c| c.call_vad_sensitivity)
        .unwrap_or(0.5);
    let vad_cfg = VadConfig { sensitivity, ..Default::default() };
    let capture = app.state::<AppState>().audio_capture.clone();
    let playback = app.state::<AppState>().audio_playback.clone();
    // No pill for call-mode turns — everything renders in the Conversation page.
    let bridge = crate::adapters::TurnEventBridge::new(app.clone(), false);

    // Reason reported when the loop exits.
    let mut ended = "hangup";

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

        // ── THINK + SPEAK (mic already off; run_turn plays synchronously) ──
        // Errors surface through the bridge; keep the call alive on failure so
        // a transient hiccup doesn't hang up. Then loop back to LISTEN.
        let _ = execute_turn(&app, transcript, None, &bridge).await;

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
