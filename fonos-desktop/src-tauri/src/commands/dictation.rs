//! Tauri commands for voice dictation.
//!
//! Simple flow: record locally → save WAV → HTTP POST → get transcript.
//! No WebSocket streaming — avoids model contention and is faster.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::audio::capture::AudioCapture;
use crate::injection::inject_text;
use tauri::{Emitter, Manager};

use super::AppState;

/// Prevents duplicate start/stop calls from rapid hotkey events.
static IS_RECORDING: AtomicBool = AtomicBool::new(false);

/// Check if currently recording.
pub fn is_recording() -> bool {
    IS_RECORDING.load(Ordering::SeqCst)
}

/// Force-reset the recording flag. Called when hiding panels to prevent
/// a stale IS_RECORDING=true from blocking future recording sessions.
pub fn force_reset_recording() {
    let was_recording = IS_RECORDING.swap(false, Ordering::SeqCst);
    if was_recording {
        eprintln!("fonos: force-reset IS_RECORDING (was stuck at true)");
    }
}

/// Move float pill back to the primary monitor (bottom center).
pub fn move_float_to_primary_pub(app: &tauri::AppHandle) {
    move_float_to_monitor(app, true);
}

/// Position the float pill at bottom-center of a monitor.
/// On macOS: uses CGEvent for cursor position and Dock clearance.
/// On Linux: uses the primary monitor, centered at bottom.
#[cfg(target_os = "macos")]
fn move_float_to_monitor(app: &tauri::AppHandle, primary: bool) {
    let Some(float_win) = app.get_webview_window("float") else { return };

    let monitors = match float_win.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return,
    };

    let target = if primary {
        monitors.iter().find(|m| m.position().x == 0 && m.position().y == 0)
            .unwrap_or(&monitors[0])
    } else {
        // CGEvent.location() returns points (logical coords on macOS).
        // Tauri monitor position/size are physical pixels.
        // Convert monitor bounds to logical for comparison.
        let cursor = {
            let source = core_graphics::event_source::CGEventSource::new(
                core_graphics::event_source::CGEventSourceStateID::CombinedSessionState
            ).expect("CGEventSource");
            let event = core_graphics::event::CGEvent::new(source).expect("CGEvent");
            event.location()
        };

        eprintln!("[fonos] cursor at logical ({:.0}, {:.0})", cursor.x, cursor.y);

        monitors.iter().find(|m| {
            let scale = m.scale_factor();
            // Convert physical → logical
            let lx = m.position().x as f64 / scale;
            let ly = m.position().y as f64 / scale;
            let lw = m.size().width as f64 / scale;
            let lh = m.size().height as f64 / scale;
            eprintln!("[fonos] monitor logical: ({:.0},{:.0}) {:.0}x{:.0} scale={scale}", lx, ly, lw, lh);
            cursor.x >= lx && cursor.x < lx + lw && cursor.y >= ly && cursor.y < ly + lh
        }).unwrap_or_else(|| {
            eprintln!("[fonos] cursor not found in any monitor, using first");
            &monitors[0]
        })
    };

    let scale = target.scale_factor();
    let pill_w = 90.0; // logical pixels
    let pill_h = 28.0;
    // ~110pt above screen bottom to clear macOS Dock + gap above it
    let dock_clearance = 110.0;

    // Convert monitor position/size to logical
    let mon_x = target.position().x as f64 / scale;
    let mon_y = target.position().y as f64 / scale;
    let mon_w = target.size().width as f64 / scale;
    let mon_h = target.size().height as f64 / scale;

    // Center horizontally, position above Dock at bottom
    let x = mon_x + (mon_w - pill_w) / 2.0;
    let y = mon_y + mon_h - pill_h - dock_clearance;

    // set_position expects physical pixels
    let _ = float_win.set_position(tauri::PhysicalPosition::new(
        (x * scale) as i32,
        (y * scale) as i32,
    ));
}

/// Linux/Windows fallback: center on primary monitor, near bottom.
/// Uses physical pixels directly (Tauri on Linux reports physical).
#[cfg(not(target_os = "macos"))]
fn move_float_to_monitor(app: &tauri::AppHandle, _primary: bool) {
    let Some(float_win) = app.get_webview_window("float") else { return };
    let monitors = match float_win.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return,
    };
    let target = &monitors[0];
    let scale = target.scale_factor();
    // Use physical pixels for position (Tauri Linux uses physical coords)
    let mon_x = target.position().x as f64;
    let mon_y = target.position().y as f64;
    let mon_w = target.size().width as f64;
    let mon_h = target.size().height as f64;
    let pill_w = 90.0 * scale;
    let pill_h = 28.0 * scale;
    let taskbar = 48.0 * scale; // approximate Linux taskbar height
    let x = mon_x + (mon_w - pill_w) / 2.0;
    let y = mon_y + mon_h - pill_h - taskbar;
    eprintln!("fonos: float pill position: ({}, {}) monitor: {}x{} scale={}", x, y, mon_w, mon_h, scale);
    let _ = float_win.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
}

#[derive(Serialize)]
pub struct SttResult {
    pub text: String,
    pub audio_path: String,
    pub latency_ms: u64,
    pub duration_secs: f64,
    /// For Apple Speech: "on-device" or "server". Empty for HTTP providers.
    pub stt_engine: String,
    /// STT backend identifier used for this transcription (for latency stats).
    pub stt_model: String,
    /// Low-frequency noise removed by high-pass filter, as percentage of total energy.
    pub noise_removed_pct: f64,
    /// Normalization gain applied in dB (positive = amplified, 0 = no change).
    pub gain_db: f64,
}

/// Check if a microphone is available AND accessible (has permission).
#[tauri::command]
pub fn has_microphone() -> Result<bool, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let device = match host.default_input_device() {
        Some(d) => d,
        None => return Ok(false),
    };
    Ok(device.supported_input_configs().is_ok())
}

/// List all available audio input devices.
#[tauri::command]
pub fn list_audio_inputs() -> Result<Vec<String>, String> {
    Ok(crate::audio::capture::list_input_devices())
}

/// Warm-up debounce: at most one backend ping per interval.
static LAST_WARMUP: AtomicU64 = AtomicU64::new(0);
const WARMUP_INTERVAL_SECS: u64 = 180;

fn is_local_endpoint(base_url: &str) -> bool {
    base_url.contains("localhost") || base_url.contains("127.0.0.1") || base_url.contains("0.0.0.0")
}

/// Fire-and-forget warm-up of the configured STT (and, for LLM modes, LLM)
/// backend so the first capture after idle doesn't pay a model cold start.
/// The ping runs while the user is speaking, so by stop time the model is
/// loaded. Local endpoints only — cloud APIs don't cold-start and probes
/// would cost money. Debounced to once per WARMUP_INTERVAL_SECS.
fn spawn_backend_warmup(state: &tauri::State<'_, AppState>) {
    let (enabled, mode_name) = match state.config.lock() {
        Ok(cfg) => (cfg.warmup_enabled, cfg.dictation_mode.clone()),
        Err(_) => return,
    };
    if !enabled {
        return;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(LAST_WARMUP.load(Ordering::Relaxed)) < WARMUP_INTERVAL_SECS {
        return;
    }
    LAST_WARMUP.store(now, Ordering::Relaxed);

    let stt = super::get_service_config(state, "stt");
    let llm = super::get_service_config(state, "llm");
    let mode_has_llm = fonos_core::modes::all_modes()
        .get(&mode_name)
        .map(|m| m.system.is_some() || m.user_template.is_some())
        .unwrap_or(false);

    tauri::async_runtime::spawn(async move {
        if stt.provider != "apple" && !stt.base_url.trim().is_empty() && is_local_endpoint(&stt.base_url) {
            let model = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
            eprintln!("fonos: warm-up ping → STT {} ({})", stt.base_url, model);
            let bytes = silent_probe_wav();
            let _ = if stt.stt_api == "chat" {
                transcribe_chat(&stt, &bytes, "", &[]).await
            } else {
                transcribe_http(&stt, &bytes, &model, "", None, &[]).await
            };
        }
        if mode_has_llm && !llm.base_url.trim().is_empty() && is_local_endpoint(&llm.base_url) {
            eprintln!("fonos: warm-up ping → LLM {} ({})", llm.base_url, llm.model);
            let base = llm.base_url.trim_end_matches('/');
            let url = if base.ends_with("/v1") {
                format!("{base}/chat/completions")
            } else {
                format!("{base}/v1/chat/completions")
            };
            let client = reqwest::Client::new();
            let _ = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", llm.api_key))
                .json(&serde_json::json!({
                    "model": llm.model,
                    "messages": [{"role": "user", "content": "ping"}],
                    "max_tokens": 1,
                }))
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await;
        }
    });
}

/// A 0.3s silent 16kHz mono WAV, used as a warm-up / endpoint-probe clip.
fn silent_probe_wav() -> Vec<u8> {
    let sample_rate = 16000u32;
    let sample_count = (sample_rate as usize) * 3 / 10;
    let pcm = vec![0u8; sample_count * 2]; // i16 LE silence
    let dir = std::env::temp_dir().join("fonos_audio");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("stt_probe.wav");
    if fonos_core::audio::write_wav(&path, &pcm, sample_rate).is_ok() {
        std::fs::read(&path).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Start capturing audio from the microphone (local only, no network).
/// When `skip_float` is true, the float pill is not moved or activated (used by agent hotkey).
#[tauri::command]
pub async fn start_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, skip_float: Option<bool>) -> Result<(), String> {
    eprintln!("fonos: start_recording called, skip_float={:?}", skip_float);

    // Read selected device from config.
    let device_name = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.audio_input_device.clone()
    };

    // Check device availability up front. These probes read no shared state, so
    // they run outside the recording-state critical section below.
    use crate::audio::capture::find_input_device;
    match find_input_device(&device_name) {
        None => {
            return Err("No microphone found. Connect an audio input device.".into());
        }
        Some(dev) => {
            use cpal::traits::DeviceTrait;
            if dev.supported_input_configs().is_err() {
                return Err("Microphone permission denied. Grant access in System Settings > Privacy > Microphone.".into());
            }
        }
    }

    // Preload the backends while the user speaks (issue #4 warm-up).
    spawn_backend_warmup(&state);

    // Critical section: hold the capture lock while we both check the recording
    // flag and bring the stream up. `stop_recording` flips the same flag under
    // the same lock, so the two can't interleave — this prevents a fast tap
    // (down then up) from draining an empty capture and then leaving the mic
    // live with IS_RECORDING=false. The flag is set to true only *after* the
    // stream is running, so the invariant "IS_RECORDING == there is a live
    // capture" always holds.
    let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;

    if IS_RECORDING.load(Ordering::SeqCst) {
        eprintln!("fonos: start_recording ignored — already recording");
        return Ok(()); // Already recording — ignore duplicate
    }

    // Always create a fresh AudioCapture — the old one may reference a
    // disconnected device or the user may have changed the device setting.
    *guard = None;
    let capture = AudioCapture::with_device(&device_name)
        .map_err(|e| format!("mic init failed: {e}"))?;
    *guard = Some(capture);
    guard.as_mut().unwrap().start()
        .map_err(|e| format!("mic start failed: {e}"))?;

    // Stream is live — now mark recording, still holding the lock.
    IS_RECORDING.store(true, Ordering::SeqCst);
    drop(guard);

    // Move float pill to the monitor where the cursor is (skip for agent mode)
    if !skip_float.unwrap_or(false) {
        move_float_to_monitor(&app, false);
        let _ = app.emit("float:start", "");
    }

    Ok(())
}

/// Silently clear the recording flag. Used by the hands-free call loop, which
/// drives capture directly (draining chunks for VAD) instead of going through
/// `stop_recording`, so it needs to reset the flag without the "was stuck"
/// warning `force_reset_recording` prints.
pub(crate) fn clear_recording_flag() {
    IS_RECORDING.store(false, Ordering::SeqCst);
}

/// A zeroed [`SttResult`] (empty transcript). Returned by the no-op stop paths.
fn empty_stt_result() -> SttResult {
    SttResult {
        text: String::new(),
        audio_path: String::new(),
        latency_ms: 0,
        duration_secs: 0.0,
        stt_engine: String::new(),
        stt_model: String::new(),
        noise_removed_pct: 0.0,
        gain_db: 0.0,
    }
}

/// Stop recording, save WAV, transcribe via HTTP API, inject text at cursor.
#[tauri::command]
pub async fn stop_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, mode_override: Option<String>) -> Result<SttResult, String> {
    // 1. Stop mic and drain all samples. The IS_RECORDING flag is flipped under
    //    the same lock start_recording uses, so a concurrent start/stop pair
    //    can't interleave (see the invariant note in start_recording).
    let all_samples: Vec<i16> = {
        let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
        if !IS_RECORDING.swap(false, Ordering::SeqCst) {
            // Not recording — nothing to stop.
            return Ok(empty_stt_result());
        }
        match guard.as_mut() {
            Some(capture) => {
                capture.stop();
                let mut samples = Vec::new();
                while let Some(chunk) = capture.take_chunk(200) {
                    samples.extend_from_slice(&chunk);
                }
                samples
            }
            None => Vec::new(),
        }
    };

    transcribe_samples(&app, state.inner(), all_samples, mode_override).await
}

/// Preprocess, save, transcribe (Apple / Whisper-HTTP / chat) and record a set
/// of already-captured 16 kHz mono i16 samples, returning the transcript and
/// metrics. Shared by [`stop_recording`] (which drains the live capture first)
/// and the hands-free call loop (which accumulates chunks itself while running
/// VAD), so both take exactly the same STT + vocab + stats + storage path.
/// `mode_override` selects the STT profile and gates the float-pill lifecycle
/// just as before ("sts-page" and "agent" stay silent).
pub(crate) async fn transcribe_samples(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
    mode_override: Option<String>,
) -> Result<SttResult, String> {
    let stop_time = std::time::Instant::now();

    // Immediately signal the float pill to switch from recording → processing.
    // Conversation-page turns never touch the pill (they render in-app).
    if mode_override.as_deref() != Some("sts-page") {
        let _ = app.emit("float:processing", ());
    }

    if all_samples.is_empty() {
        return Ok(empty_stt_result());
    }

    // Pure preprocess + STT (+ vocab) via the shared core; this function layers
    // the float-pill, injection, stats, and storage side effects on top.
    let SttCore {
        transcript,
        stt_engine,
        audio_path: audio_path_str,
        stats_model,
        dictation_mode,
        mode_has_llm,
        recording_duration,
        preprocess_metrics,
    } = transcribe_core(app, state, all_samples, mode_override, true).await?;

    let latency_ms = stop_time.elapsed().as_millis() as u64;

    // Raw dictations complete inside this function (transcribe + inject), so
    // their end-to-end latency is recorded here; LLM modes are recorded by the
    // hotkey pipeline after processing + injection.
    let mut raw_e2e_done = false;

    // 4. Notify float window (stops the recording animation) — skip for agent mode
    eprintln!("fonos: stop_recording dictation_mode='{}' transcript_len={}", dictation_mode, transcript.len());
    if !matches!(dictation_mode.as_str(), "agent" | "sts-page") {
        // `mode_has_llm` (from the core): when set, the STT transcript is NOT the
        // final output — the LLM caller (the hotkey pipelines in main.rs, or the
        // Dictation view) emits the real float:stop/float:error after LLM +
        // injection completes.

        // 5. Inject raw text only in raw mode (not agent, not LLM modes).
        // Injection runs BEFORE the success emit: on failure the pill must
        // show only the error — a float:stop first would schedule the pill's
        // revert-to-idle timer, which would cut the error display short.
        let mut delivered = true;
        if dictation_mode == "raw" && !transcript.is_empty() {
            eprintln!("fonos: INJECTING raw text at cursor ({} chars)", transcript.len());
            let inj_cfg = state.config.lock().map(|c| c.clone()).unwrap_or_default();
            if let Err(e) = inject_text(&transcript, &inj_cfg) {
                delivered = false;
                let msg = format!("Injection failed: {e}");
                crate::error_surface::emit_float_error(app, &msg);
            }
            raw_e2e_done = delivered;
        }
        if delivered {
            if mode_has_llm && !transcript.is_empty() {
                // Pipeline isn't done: the transcript still goes to the LLM.
                // Keep the pill in "Processing" and let the LLM caller emit the
                // final float:stop — emitting float:stop here would flash a
                // premature green "Done" before the LLM step even runs.
                let _ = app.emit("float:processing", ());
            } else {
                let _ = app.emit("float:stop", &transcript);
            }
        }
    } else {
        eprintln!("fonos: {dictation_mode} mode — skipping float:stop and inject");
    }

    // Record STT event to stats DB (legacy). `stats_model` comes from the core.
    if !transcript.is_empty() {
        if let Ok(db) = state.db.lock() {
            let _ = fonos_core::stats::record_event(
                &db, "stt", &transcript, "", recording_duration,
                latency_ms as i64, &dictation_mode, &stats_model, "", &audio_path_str,
                0, 0, "",
            );

            // End-to-end latency (key release → injected) for raw dictations.
            if raw_e2e_done {
                let _ = fonos_core::stats::record_dictation_latency(
                    &db, stop_time.elapsed().as_millis() as i64, &dictation_mode, &stats_model,
                );
            }

            // Write to v2 unified entries table — all activity is recorded.
            // Recent view shows everything; Notes view filters to note-only.
            let source = match dictation_mode.as_str() {
                "agent" | "sts-page" => fonos_core::storage::SourceType::Agent,
                "note" => fonos_core::storage::SourceType::Note,
                _ => fonos_core::storage::SourceType::Dictation,
            };
            let entry = fonos_core::storage::Entry {
                id: None,
                created_at: crate::commands::storage::now_iso8601(),
                source_type: source,
                role: fonos_core::storage::EntryRole::User,
                mode: dictation_mode.clone(),
                raw_text: transcript.clone(),
                processed_text: None,
                container_id: if dictation_mode == "note" {
                    // Use the notebook selected in the note panel (stored in AppState).
                    // If no target set (race condition on first open), fall back to Quick Note.
                    let target = state.note_target.lock().ok().and_then(|g| *g);
                    if target.is_some() {
                        target
                    } else {
                        // Find Quick Note container as fallback
                        db.query_row(
                            "SELECT id FROM containers WHERE container_type='notebook' AND title='Quick Note' LIMIT 1",
                            [], |r| r.get::<_, i64>(0)
                        ).ok()
                    }
                } else {
                    None
                },
                audio_ref: if dictation_mode == "note" { None } else { Some(audio_path_str.clone()) },
                metadata: serde_json::json!({
                    "duration_secs": recording_duration,
                    "latency_ms": latency_ms,
                }),
            };
            if let Err(e) = fonos_core::storage::insert_entry(&db, &entry) {
                eprintln!("fonos: entry write error: {e}");
            }
        }
    }

    Ok(SttResult {
        text: transcript,
        audio_path: audio_path_str,
        latency_ms,
        duration_secs: recording_duration,
        stt_engine,
        stt_model: stats_model,
        noise_removed_pct: preprocess_metrics.0,
        gain_db: preprocess_metrics.1,
    })
}

/// The pure preprocess + STT product, before any float-pill / stats / storage /
/// injection side effects — returned by [`transcribe_core`] and layered on by
/// its callers.
struct SttCore {
    transcript: String,
    /// Apple Speech engine tag ("on-device"/"server"), empty for HTTP providers.
    stt_engine: String,
    /// Path to the saved WAV (kept for history / stats `audio_ref`).
    audio_path: String,
    /// Model label for stats (`"Apple Speech (…)"` or the HTTP model name).
    stats_model: String,
    /// Effective dictation mode after resolving `mode_override`.
    dictation_mode: String,
    /// Whether the resolved mode runs a downstream LLM step.
    mode_has_llm: bool,
    recording_duration: f64,
    /// `(noise_removed_pct, gain_db)` from preprocessing.
    preprocess_metrics: (f64, f64),
}

/// Preprocess (high-pass + normalize) and transcribe a set of already-captured
/// 16 kHz mono i16 samples via the resolved STT profile (Apple / Whisper-HTTP /
/// chat), optionally applying the deterministic vocab post-correction. This is
/// the pure STT step shared by [`transcribe_samples`] (which then records stats,
/// storage, and drives the float pill / injection) and the call-mode barge
/// content verifier ([`transcribe_stt_only`], which wants only the transcript).
///
/// No stats, storage, or float-pill side effects of its own; an STT *error* is
/// still surfaced to the float pill for interactive modes (not "agent"/"sts-page"),
/// exactly as before. `mode_override` selects the STT profile just as in
/// [`transcribe_samples`]; `apply_vocab` runs the vocab rules (the verifier skips
/// them for speed).
async fn transcribe_core(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
    mode_override: Option<String>,
    apply_vocab: bool,
) -> Result<SttCore, String> {
    let recording_duration = all_samples.len() as f64 / 16000.0;

    // 1b. Audio preprocessing: high-pass filter + RMS normalization
    let (all_samples, preprocess_metrics) = preprocess_audio(all_samples);

    // 2. Save WAV for history + transcription
    let audio_dir = std::env::temp_dir().join("fonos_audio");
    let _ = std::fs::create_dir_all(&audio_dir);
    let audio_path = audio_dir.join(format!("stt_{}.wav", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()));

    let pcm_bytes: Vec<u8> = all_samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    fonos_core::audio::write_wav(&audio_path, &pcm_bytes, 16000)
        .map_err(|e| e.to_string())?;
    let audio_path_str = audio_path.to_string_lossy().to_string();

    // 3. Transcribe via HTTP (single call, no contention from streaming partials)
    let file_bytes = std::fs::read(&audio_path)
        .map_err(|e| format!("failed to read WAV: {e}"))?;

    // Load config + mode to determine which STT profile to use
    // mode_override (from Dictation view) takes precedence over config.dictation_mode (float pill)
    let (dictation_mode, stt_language) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let mode = mode_override.unwrap_or_else(|| config.dictation_mode.clone());
        (mode, config.stt_language.clone())
    };
    let all_modes = fonos_core::modes::all_modes();
    let current_mode = all_modes.get(&dictation_mode);

    // Resolve effective vocab books for this dictation (global ∪ mode, issue #3).
    // Cloned out of the config lock so the books outlive the transcription awaits.
    let vocab_books: Vec<fonos_core::vocab::VocabBook> = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let empty: &[String] = &[];
        let mode_ids = current_mode.map(|m| m.vocab_books.as_slice()).unwrap_or(empty);
        fonos_core::vocab::effective_books(&config.vocab_books, &config.global_vocab_books, mode_ids)
            .into_iter()
            .cloned()
            .collect()
    };
    let vocab_refs: Vec<&fonos_core::vocab::VocabBook> = vocab_books.iter().collect();
    let vocab_terms = fonos_core::vocab::collect_terms(&vocab_refs);

    // Read STT config — check for Apple Speech sentinel, then mode override, then global default
    let stt = match current_mode {
        Some(mode) if mode.stt_model == "apple-speech" => {
            eprintln!("fonos: STT using Apple Speech on-device (mode={})", dictation_mode);
            super::ServiceConfig {
                base_url: String::new(), api_key: String::new(),
                model: "apple-speech".to_string(), provider: "apple".to_string(),
                stt_api: "whisper".to_string(),
            }
        }
        Some(mode) if !mode.stt_model.is_empty() => {
            eprintln!("fonos: STT using mode override profile '{}' (mode={})", mode.stt_model, dictation_mode);
            super::get_service_config_for_profile(&state, &mode.stt_model)
        }
        _ => {
            eprintln!("fonos: STT using global default profile (mode={})", dictation_mode);
            super::get_service_config(&state, "stt")
        }
    };
    eprintln!("fonos: STT provider={} endpoint={} model={}", stt.provider, stt.base_url, stt.model);

    // Convert language name to ISO 639-1 / BCP-47 code
    let lang_code = {
        let lang = stt_language.trim();
        if lang.is_empty() || lang == "auto" { String::new() } else {
            match lang.to_lowercase().as_str() {
                "chinese"    => "zh",
                "english"    => "en",
                "japanese"   => "ja",
                "korean"     => "ko",
                "cantonese"  => "yue",
                "french"     => "fr",
                "german"     => "de",
                "spanish"    => "es",
                "portuguese" => "pt",
                "italian"    => "it",
                "russian"    => "ru",
                "arabic"     => "ar",
                "hindi"      => "hi",
                "thai"       => "th",
                "vietnamese" => "vi",
                "dutch"      => "nl",
                "polish"     => "pl",
                "turkish"    => "tr",
                "indonesian" => "id",
                other        => other,
            }.to_string()
        }
    };

    let model_name = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };

    // ── Branch: Apple on-device / Whisper API / Chat Completions STT ────────
    let (transcript, stt_engine) = if stt.provider == "apple" {
        transcribe_apple(&audio_path_str, &lang_code, &vocab_terms).await
    } else {
        let result = if stt.stt_api == "chat" {
            eprintln!("fonos: STT via chat completions (base64 audio)");
            transcribe_chat(&stt, &file_bytes, &lang_code, &vocab_terms).await
        } else {
            transcribe_http(&stt, &file_bytes, &model_name, &lang_code, current_mode, &vocab_terms).await
        };
        match result {
            Ok(t) => (t, String::new()),
            Err(e) => {
                // Surface the failure instead of returning an empty transcript
                // silently. The float pill shows the error and leaves processing
                // state; the caller (hotkey / Dictation view) also gets the Err.
                let msg = format!("STT failed via {} at {}: {}", stt.provider, stt.base_url, e);
                if !matches!(dictation_mode.as_str(), "agent" | "sts-page") {
                    crate::error_surface::emit_float_error(app, &msg);
                } else {
                    eprintln!("fonos: {msg}");
                }
                return Err(msg);
            }
        }
    };

    // ② Deterministic post-correction from the effective vocab books — runs
    // for every mode (raw, LLM, note, agent) before any downstream use.
    let transcript = if !apply_vocab || vocab_refs.is_empty() {
        transcript
    } else {
        fonos_core::vocab::apply_rules(&transcript, &vocab_refs)
    };

    let mode_has_llm = current_mode
        .map(|m| m.system.is_some() || m.user_template.is_some())
        .unwrap_or(false);

    let stats_model = if stt.provider == "apple" {
        format!("Apple Speech ({})", if stt_engine.is_empty() { "server" } else { &stt_engine })
    } else {
        model_name.clone()
    };

    Ok(SttCore {
        transcript,
        stt_engine,
        audio_path: audio_path_str,
        stats_model,
        dictation_mode,
        mode_has_llm,
        recording_duration,
        preprocess_metrics,
    })
}

/// STT-only transcription for the call-mode barge content verifier: preprocess +
/// transcribe and return just the transcript. No vocab pass (speed), and no
/// stats, storage, float-pill, or injection side effects. Uses the same STT
/// profile the call's listen phase uses (mode "sts-page" → global default), so
/// the snippet is transcribed exactly as a normal listen would transcribe it.
pub(crate) async fn transcribe_stt_only(
    app: &tauri::AppHandle,
    state: &AppState,
    all_samples: Vec<i16>,
) -> Result<String, String> {
    if all_samples.is_empty() {
        return Ok(String::new());
    }
    let core = transcribe_core(app, state, all_samples, Some("sts-page".to_string()), false).await?;
    Ok(core.transcript)
}

/// Transcribe an audio file via POST /v1/audio/transcriptions.
#[tauri::command]
pub async fn transcribe_file(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let file_bytes = tokio::fs::read(&path).await
        .map_err(|e| format!("failed to read '{path}': {e}"))?;

    let file_name = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let stt = super::get_service_config(&state, "stt");
    let model_name = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model_name);
    let url = format!("{}/v1/audio/transcriptions", stt.base_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build().map_err(|e| e.to_string())?;

    let mut req = client.post(&url).multipart(form);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    let response = req.send().await
        .map_err(|e| format!("transcription failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("transcription error {status}: {body}"));
    }

    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("parse error: {e}"))?;

    Ok(json["text"].as_str().unwrap_or("").to_string())
}

// ─── STT provider helpers ────────────────────────────────────────────────────

/// Transcribe via macOS SFSpeechRecognizer (calls the bundled Swift helper).
/// Returns (transcript, engine) where engine is "on-device" or "server".
async fn transcribe_apple(audio_path: &str, lang_code: &str, vocab_terms: &[String]) -> (String, String) {
    // Find the helper binary next to the app binary or in resources
    let helper = find_apple_stt_binary();
    let Some(helper_path) = helper else {
        eprintln!("fonos: fonos-stt-apple binary not found — cannot use Apple Speech");
        return (String::new(), String::new());
    };

    // Map ISO 639-1 → BCP-47 locale for Apple Speech
    let locale = match lang_code {
        "zh" => "zh-CN",
        "en" => "en-US",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "es" => "es-ES",
        "pt" => "pt-BR",
        "it" => "it-IT",
        "ru" => "ru-RU",
        "ar" => "ar-SA",
        "th" => "th-TH",
        "vi" => "vi-VN",
        "nl" => "nl-NL",
        "pl" => "pl-PL",
        "tr" => "tr-TR",
        "id" => "id-ID",
        other if !other.is_empty() => other,
        _ => "en-US",
    };

    eprintln!("fonos: Apple STT transcribing {} (locale={})", audio_path, locale);

    let mut cmd = tokio::process::Command::new(&helper_path);
    cmd.arg(audio_path).arg(locale);
    // Vocabulary biasing via SFSpeechRecognizer contextualStrings.
    if !vocab_terms.is_empty() {
        if let Ok(json) = serde_json::to_string(vocab_terms) {
            cmd.arg(json);
        }
    }
    let output = cmd.output().await;

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("fonos: Apple STT failed: {}{}", stdout.trim(), stderr.trim());
                return (String::new(), String::new());
            }
            let json: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_default();
            if let Some(err) = json["error"].as_str() {
                eprintln!("fonos: Apple STT error: {}", err);
                return (String::new(), String::new());
            }
            let text = json["text"].as_str().unwrap_or("").to_string();
            let engine = json["engine"].as_str().unwrap_or("server").to_string();
            let on_device = json["on_device_available"].as_bool().unwrap_or(false);
            eprintln!("fonos: Apple STT [{}] (on-device available: {}): {}",
                engine, on_device, text.chars().take(80).collect::<String>());
            (text, engine)
        }
        Err(e) => {
            eprintln!("fonos: failed to run fonos-stt-apple: {e}");
            (String::new(), String::new())
        }
    }
}

/// Locate the fonos-stt-apple binary.
fn find_apple_stt_binary() -> Option<String> {
    let name = "fonos-stt-apple";
    let candidates: Vec<std::path::PathBuf> = {
        let mut v = Vec::new();
        // 1. Next to current executable (covers `cargo run` from target/debug/)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                v.push(dir.join(name));
                // 2. macOS .app bundle: Contents/MacOS/../Resources/
                if let Some(parent) = dir.parent() {
                    v.push(parent.join("Resources").join(name));
                }
            }
        }
        // 3. Development paths relative to CWD
        v.push(std::path::PathBuf::from(format!("src-tauri/resources/{name}")));
        v.push(std::path::PathBuf::from(format!("fonos-desktop/src-tauri/resources/{name}")));
        v
    };
    for c in &candidates {
        if c.exists() {
            eprintln!("fonos: found Apple STT binary at {}", c.display());
            return Some(c.to_string_lossy().to_string());
        }
    }
    eprintln!("fonos: searched for {name} in: {:?}", candidates.iter().map(|c| c.display().to_string()).collect::<Vec<_>>());
    None
}

// STT clients moved to fonos-core (issue #21); re-exported so existing
// call sites (meeting.rs, warm-up, test_stt) keep their paths.
pub use fonos_core::stt::{transcribe_chat, transcribe_http};

// ─── STT endpoint verification ───────────────────────────────────────────────

/// Verify a model's STT endpoint by sending a short silent probe clip.
///
/// Distinguishes "endpoint works" (any 2xx, even an empty transcript from
/// silence) from "endpoint missing/broken" (404/401/network). Lets users
/// confirm a self-hosted STT server before relying on it, instead of finding
/// out via a silent empty dictation.
#[tauri::command]
pub async fn test_stt(state: tauri::State<'_, AppState>, profile_id: String) -> Result<String, String> {
    let stt = super::get_service_config_for_profile(&state, &profile_id);

    if stt.provider == "apple" {
        return Ok("Apple on-device speech — no network endpoint to test.".to_string());
    }
    if stt.base_url.trim().is_empty() {
        return Err("This model has no base URL configured.".to_string());
    }

    // Short silent WAV probe clip (shared with the warm-up path).
    let bytes = silent_probe_wav();
    if bytes.is_empty() {
        return Err("failed to build probe clip".to_string());
    }

    let model_name = if stt.model.is_empty() { "fast".to_string() } else { stt.model.clone() };
    let result = if stt.stt_api == "chat" {
        transcribe_chat(&stt, &bytes, "", &[]).await
    } else {
        transcribe_http(&stt, &bytes, &model_name, "", None, &[]).await
    };

    match result {
        Ok(_) => Ok(format!("OK — {} responded at {}", stt.provider, stt.base_url)),
        Err(e) => Err(e),
    }
}

// ─── Audio preprocessing ─────────────────────────────────────────────────────

/// Apply speech-optimized preprocessing: high-pass filter (80Hz) + RMS normalization.
/// Returns (processed_samples, (noise_removed_pct, gain_db)).
fn preprocess_audio(samples: Vec<i16>) -> (Vec<i16>, (f64, f64)) {
    if samples.is_empty() { return (samples, (0.0, 0.0)); }

    // 1. High-pass filter at 80Hz (first-order IIR)
    let alpha: f64 = 1.0 / (1.0 + 2.0 * std::f64::consts::PI * 80.0 / 16000.0);
    let mut filtered = Vec::with_capacity(samples.len());
    let mut prev_in: f64 = samples[0] as f64;
    let mut prev_out: f64 = samples[0] as f64;
    filtered.push(samples[0]);
    for &s in &samples[1..] {
        let x = s as f64;
        let y = alpha * (prev_out + x - prev_in);
        prev_in = x;
        prev_out = y;
        filtered.push(y.round().clamp(-32768.0, 32767.0) as i16);
    }

    // Measure noise removed: energy difference between original and filtered
    let energy_orig: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let energy_removed: f64 = samples.iter().zip(filtered.iter())
        .map(|(&a, &b)| { let d = a as f64 - b as f64; d * d }).sum();
    let noise_removed_pct = if energy_orig > 0.0 { 100.0 * energy_removed / energy_orig } else { 0.0 };

    // 2. RMS normalization to -20 dBFS (target ~3277)
    let rms: f64 = {
        let sum_sq: f64 = filtered.iter().map(|&s| (s as f64) * (s as f64)).sum();
        (sum_sq / filtered.len() as f64).sqrt()
    };

    if rms < 1.0 {
        eprintln!("fonos: audio preprocessing: silence detected, skipping normalization");
        return (filtered, (noise_removed_pct, 0.0));
    }

    let target_rms: f64 = 3277.0;
    let gain = (target_rms / rms).min(10.0);
    let gain_db = 20.0 * gain.log10();

    let normalized: Vec<i16> = filtered.iter().map(|&s| {
        let v = (s as f64 * gain).round();
        v.clamp(-32768.0, 32767.0) as i16
    }).collect();

    eprintln!("fonos: audio preprocessing: HPF removed {:.1}% noise, normalized {:.1}dB",
        noise_removed_pct, gain_db);

    (normalized, (noise_removed_pct, gain_db))
}
