//! Tauri commands for stats & history.

use super::AppState;
use fonos_core::stats;

#[tauri::command]
pub fn record_event(
    state: tauri::State<'_, AppState>,
    event_type: String,
    input_text: String,
    output_text: String,
    duration_secs: f64,
    latency_ms: i64,
    mode: String,
    model: String,
    voice: String,
    audio_path: String,
    tokens_in: Option<i64>,
    tokens_out: Option<i64>,
    session_id: Option<String>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::record_event(
        &conn,
        &event_type, &input_text, &output_text,
        duration_secs, latency_ms,
        &mode, &model, &voice, &audio_path,
        tokens_in.unwrap_or(0), tokens_out.unwrap_or(0),
        &session_id.unwrap_or_default(),
    ).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_event(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::delete_event(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stats(
    state: tauri::State<'_, AppState>,
    date_from: String,
    date_to: String,
) -> Result<Vec<stats::DailyStat>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::get_daily_stats(&conn, &date_from, &date_to).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_history(
    state: tauri::State<'_, AppState>,
    limit: i64,
    offset: i64,
    type_filter: String,
) -> Result<Vec<stats::Event>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::get_history(&conn, limit, offset, &type_filter).map_err(|e| e.to_string())
}

/// End-to-end dictation latency percentiles over a date window (issue #4).
#[tauri::command(rename_all = "snake_case")]
pub fn get_dictation_latency(
    state: tauri::State<'_, AppState>,
    date_from: String,
    date_to: String,
) -> Result<stats::LatencyStats, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::get_dictation_latency(&conn, &date_from, &date_to).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_today(
    state: tauri::State<'_, AppState>,
) -> Result<stats::TodaySummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    stats::get_today(&conn).map_err(|e| e.to_string())
}
