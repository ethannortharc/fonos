//! Tauri commands for voice dictation.
//!
//! Simple flow: record locally → save WAV → HTTP POST → get transcript.
//! No WebSocket streaming — avoids model contention and is faster.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Move float pill to the monitor where the cursor is (bottom center).
pub fn move_float_to_cursor_pub(app: &tauri::AppHandle) {
    move_float_to_monitor(app, false);
}

/// Move float pill back to the primary monitor (bottom center).
pub fn move_float_to_primary_pub(app: &tauri::AppHandle) {
    move_float_to_monitor(app, true);
}

/// Position the float pill at bottom-center of a monitor, above the Dock.
/// If `primary` is true, uses the primary monitor. Otherwise, uses the monitor
/// where the mouse cursor currently is.
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

#[derive(Serialize)]
pub struct SttResult {
    pub text: String,
    pub audio_path: String,
    pub latency_ms: u64,
    pub duration_secs: f64,
    /// For Apple Speech: "on-device" or "server". Empty for HTTP providers.
    pub stt_engine: String,
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

/// Start capturing audio from the microphone (local only, no network).
/// When `skip_float` is true, the float pill is not moved or activated (used by agent hotkey).
#[tauri::command]
pub async fn start_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, skip_float: Option<bool>) -> Result<(), String> {
    let was_recording = IS_RECORDING.swap(true, Ordering::SeqCst);
    eprintln!("fonos: start_recording called, was_recording={}, skip_float={:?}", was_recording, skip_float);
    if was_recording {
        return Ok(()); // Already recording — ignore duplicate
    }

    // Read selected device from config
    let device_name = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        config.audio_input_device.clone()
    };

    // Check device availability
    use crate::audio::capture::find_input_device;
    match find_input_device(&device_name) {
        None => {
            IS_RECORDING.store(false, Ordering::SeqCst);
            return Err("No microphone found. Connect an audio input device.".into());
        }
        Some(dev) => {
            use cpal::traits::DeviceTrait;
            if dev.supported_input_configs().is_err() {
                IS_RECORDING.store(false, Ordering::SeqCst);
                return Err("Microphone permission denied. Grant access in System Settings > Privacy > Microphone.".into());
            }
        }
    }

    let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;

    // Always create a fresh AudioCapture — the old one may reference a
    // disconnected device or the user may have changed the device setting.
    *guard = None;
    let capture = AudioCapture::with_device(&device_name).map_err(|e| {
        IS_RECORDING.store(false, Ordering::SeqCst);
        format!("mic init failed: {e}")
    })?;
    *guard = Some(capture);

    guard.as_mut().unwrap().start().map_err(|e| {
        IS_RECORDING.store(false, Ordering::SeqCst);
        format!("mic start failed: {e}")
    })?;

    // Move float pill to the monitor where the cursor is (skip for agent mode)
    if !skip_float.unwrap_or(false) {
        move_float_to_monitor(&app, false);
        let _ = app.emit("float:start", "");
    }

    Ok(())
}

/// Stop recording, save WAV, transcribe via HTTP API, inject text at cursor.
#[tauri::command]
pub async fn stop_recording(app: tauri::AppHandle, state: tauri::State<'_, AppState>, mode_override: Option<String>) -> Result<SttResult, String> {
    if !IS_RECORDING.swap(false, Ordering::SeqCst) {
        // Not recording — silently ignore (no log spam)
        return Ok(SttResult {
            text: String::new(),
            audio_path: String::new(),
            latency_ms: 0,
            duration_secs: 0.0,
            stt_engine: String::new(),
            noise_removed_pct: 0.0,
            gain_db: 0.0,
        });
    }

    let stop_time = std::time::Instant::now();

    // 1. Stop mic and drain all samples
    let all_samples: Vec<i16> = {
        let mut guard = state.audio_capture.lock().map_err(|e| e.to_string())?;
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

    let recording_duration = all_samples.len() as f64 / 16000.0;

    // Immediately signal the float pill to switch from recording → processing
    {
        use tauri::Emitter;
        let _ = app.emit("float:processing", ());
    }

    if all_samples.is_empty() {
        return Ok(SttResult {
            text: String::new(),
            audio_path: String::new(),
            latency_ms: 0,
            duration_secs: 0.0,
            stt_engine: String::new(),
            noise_removed_pct: 0.0,
            gain_db: 0.0,
        });
    }


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

    // Convert language to ISO 639-1 / BCP-47 code
    let lang_code = {
        let langs: Vec<&str> = stt_language.split(',').map(|s| s.trim())
            .filter(|s| !s.is_empty() && *s != "auto").collect();
        if langs.is_empty() { String::new() } else {
            let lang_lower = langs[0].to_lowercase();
            match lang_lower.as_str() {
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
        transcribe_apple(&audio_path_str, &lang_code).await
    } else if stt.stt_api == "chat" {
        eprintln!("fonos: STT via chat completions (base64 audio)");
        (transcribe_chat(&stt, &file_bytes, &lang_code).await, String::new())
    } else {
        (transcribe_http(&stt, &file_bytes, &model_name, &lang_code, current_mode).await, String::new())
    };

    let latency_ms = stop_time.elapsed().as_millis() as u64;

    // 4. Notify float window (stops the recording animation) — skip for agent mode
    eprintln!("fonos: stop_recording dictation_mode='{}' transcript_len={}", dictation_mode, transcript.len());
    if dictation_mode != "agent" {
        let _ = app.emit("float:stop", &transcript);
        // 5. Inject raw text only in raw mode (not agent, not LLM modes)
        if dictation_mode == "raw" && !transcript.is_empty() {
            eprintln!("fonos: INJECTING raw text at cursor ({} chars)", transcript.len());
            let _ = inject_text(&transcript);
        }
    } else {
        eprintln!("fonos: agent mode — skipping float:stop and inject");
    }

    // Record STT event to stats DB (legacy)
    let stats_model = if stt.provider == "apple" {
        format!("Apple Speech ({})", if stt_engine.is_empty() { "server" } else { &stt_engine })
    } else {
        model_name.clone()
    };
    if !transcript.is_empty() {
        if let Ok(db) = state.db.lock() {
            let _ = fonos_core::stats::record_event(
                &db, "stt", &transcript, "", recording_duration,
                latency_ms as i64, &dictation_mode, &stats_model, "", &audio_path_str,
                0, 0, "",
            );

            // Write to v2 unified entries table — all activity is recorded.
            // Recent view shows everything; Notes view filters to note-only.
            let source = match dictation_mode.as_str() {
                "agent" => fonos_core::storage::SourceType::Agent,
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
        noise_removed_pct: preprocess_metrics.0,
        gain_db: preprocess_metrics.1,
    })
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
async fn transcribe_apple(audio_path: &str, lang_code: &str) -> (String, String) {
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

    let output = tokio::process::Command::new(&helper_path)
        .arg(audio_path)
        .arg(locale)
        .output()
        .await;

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

/// Transcribe via HTTP POST to an OpenAI-compatible /v1/audio/transcriptions endpoint.
pub async fn transcribe_http(
    stt: &super::ServiceConfig,
    file_bytes: &[u8],
    model_name: &str,
    lang_code: &str,
    current_mode: Option<&fonos_core::modes::Mode>,
) -> String {
    let url = format!("{}/v1/audio/transcriptions", stt.base_url);
    let part = match reqwest::multipart::Part::bytes(file_bytes.to_vec())
        .file_name("recording.wav")
        .mime_str("audio/wav") {
        Ok(p) => p,
        Err(e) => { eprintln!("fonos: multipart error: {e}"); return String::new(); }
    };

    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model_name.to_string());

    // Apply mode STT params
    if let Some(mode) = current_mode {
        if !mode.stt_prompt.is_empty() {
            form = form.text("prompt", mode.stt_prompt.clone());
        }
        if mode.stt_temperature > 0.0 {
            form = form.text("temperature", mode.stt_temperature.to_string());
        }
    }

    if !lang_code.is_empty() {
        form = form.text("language", lang_code.to_string());
    }

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build() {
        Ok(c) => c,
        Err(e) => { eprintln!("fonos: http client error: {e}"); return String::new(); }
    };

    let mut req = client.post(&url).multipart(form);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            let json: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
            json["text"].as_str().unwrap_or("").to_string()
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("fonos: transcription error {status}: {body}");
            String::new()
        }
        Err(e) => {
            eprintln!("fonos: transcription failed: {e}");
            String::new()
        }
    }
}

// ─── Chat-completions-based STT (OpenRouter, Gemini, Voxtral, etc.) ─────────

/// Transcribe audio by sending it as base64 in a chat completions request.
/// This path works with multimodal models that accept `input_audio` content
/// blocks (OpenRouter, Gemini, Voxtral, GPT-Audio, etc.).
pub async fn transcribe_chat(
    stt: &super::ServiceConfig,
    file_bytes: &[u8],
    lang_code: &str,
) -> String {
    use base64::Engine;

    let url = {
        let base = stt.base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/chat/completions", base)
        } else {
            format!("{}/v1/chat/completions", base)
        }
    };

    let audio_b64 = base64::engine::general_purpose::STANDARD.encode(file_bytes);

    let lang_hint = if lang_code.is_empty() {
        String::new()
    } else {
        format!(" The audio is in language code '{}'.", lang_code)
    };

    let body = serde_json::json!({
        "model": stt.model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": format!(
                        "Transcribe this audio exactly as spoken. Output only the transcript text, nothing else.{}",
                        lang_hint
                    )
                },
                {
                    "type": "input_audio",
                    "input_audio": {
                        "data": audio_b64,
                        "format": "wav"
                    }
                }
            ]
        }],
        "temperature": 0.0,
        "max_tokens": 4096
    });

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("fonos: chat-stt client error: {e}");
            return String::new();
        }
    };

    let mut req = client.post(&url).json(&body);
    if !stt.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", stt.api_key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let json: serde_json::Value = resp.json().await.unwrap_or_default();
            json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string()
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("fonos: chat-stt error {status}: {}", body.chars().take(200).collect::<String>());
            String::new()
        }
        Err(e) => {
            eprintln!("fonos: chat-stt request failed: {e}");
            String::new()
        }
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
