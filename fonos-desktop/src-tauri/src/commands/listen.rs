//! Listen queue (issue #23): capture text → core listen workflow →
//! stored playable entry. The workflow itself lives in fonos-core; this
//! module only resolves config, owns file/db placement, and adapts events
//! onto the float pill.

use super::AppState;
use fonos_core::pipeline::{EventSink, PipelineEvent};
use tauri::Manager;

/// Hotkey entry point: grab the current selection and run the workflow.
pub async fn run_listen_capture(app: tauri::AppHandle) -> Result<i64, String> {
    let text = super::selection::grab_selection()
        .await
        .map(|s| s.text)
        .unwrap_or_default();
    let state: tauri::State<'_, AppState> = app.state();
    create_inner(&app, &state, text).await
}

/// Command entry point (e.g. from the UI): run the workflow on given text.
#[tauri::command]
pub async fn create_listen_from_text(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    text: String,
) -> Result<i64, String> {
    create_inner(&app, &state, text).await
}

async fn create_inner(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
    text: String,
) -> Result<i64, String> {
    let events = crate::adapters::PillEventSink(app.clone());
    events.emit(PipelineEvent::Processing);
    match do_create(state, &text).await {
        Ok((id, title)) => {
            eprintln!("fonos: listen item created: {title}");
            events.emit(PipelineEvent::Delivered(title));
            Ok(id)
        }
        Err(e) => {
            events.emit(PipelineEvent::Failed(fonos_core::error_class::classify_error(&e)));
            Err(e)
        }
    }
}

async fn do_create(
    state: &tauri::State<'_, AppState>,
    text: &str,
) -> Result<(i64, String), String> {
    if text.trim().is_empty() {
        return Err("No text selected — select some text, then press the Listen hotkey.".into());
    }

    let (mode_id, voice_profile, voice, translate_target) = {
        let cfg = state.config.lock().map_err(|e| e.to_string())?;
        (
            cfg.listen_mode.clone(),
            cfg.listen_voice_profile.clone(),
            cfg.listen_voice.clone(),
            cfg.translate_target.clone(),
        )
    };
    let all = fonos_core::modes::all_modes();
    let mode = all
        .get(&mode_id)
        .ok_or_else(|| format!("Listen mode '{mode_id}' not found — pick one in Settings > Speech."))?;

    let llm = if !mode.model.is_empty() {
        super::get_service_config_for_profile(state, &mode.model)
    } else {
        super::get_service_config(state, "llm")
    };
    if llm.base_url.trim().is_empty() {
        return Err("No LLM profile configured — pick one in Settings > Models.".into());
    }
    let tts_svc = if !voice_profile.is_empty() {
        super::get_service_config_for_profile(state, &voice_profile)
    } else {
        super::get_service_config(state, "tts")
    };
    if tts_svc.base_url.trim().is_empty() {
        return Err("No TTS profile configured — pick one in Settings > Speech.".into());
    }

    let engine = fonos_core::tts::HttpTts { service: tts_svc, voice, speed: 1.0 };
    let item =
        fonos_core::listen::create_listen_item(text, mode, &llm, &translate_target, &engine).await?;

    // Persist audio under the app data dir.
    let dir = fonos_core::config::AppConfig::config_dir().join("listen");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create listen dir: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("listen_{stamp}.wav"));
    std::fs::write(&path, &item.audio_wav).map_err(|e| format!("could not write audio: {e}"))?;

    // Store the entry (raw = captured text, processed = the spoken briefing).
    let entry = fonos_core::storage::Entry {
        id: None,
        created_at: super::storage::now_iso8601(),
        source_type: fonos_core::storage::SourceType::Listen,
        role: fonos_core::storage::EntryRole::User,
        mode: mode_id,
        raw_text: text.to_string(),
        processed_text: Some(item.processed.clone()),
        container_id: None,
        audio_ref: Some(path.to_string_lossy().to_string()),
        metadata: serde_json::json!({ "title": item.title }),
    };
    let id = {
        let db = state.db.lock().map_err(|e| e.to_string())?;
        fonos_core::storage::insert_entry(&db, &entry).map_err(|e| e.to_string())?
    };
    Ok((id, item.title))
}
