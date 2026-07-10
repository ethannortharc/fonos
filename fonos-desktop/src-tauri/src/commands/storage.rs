//! Tauri commands that expose the fonos-core storage layer to the React frontend.
//!
//! All commands access the shared SQLite connection via `state.db.lock()` — the
//! same pattern used by `stats.rs` and other command modules.

use super::AppState;
use fonos_core::storage::{
    self, Container, ContainerType, Entry, EntryFilter, SourceType,
};
use std::path::PathBuf;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Parse an optional source_type string into `Option<SourceType>`.
fn parse_source_type(s: &str) -> Option<SourceType> {
    match s {
        "dictation" => Some(SourceType::Dictation),
        "agent" => Some(SourceType::Agent),
        "note" => Some(SourceType::Note),
        "meeting" => Some(SourceType::Meeting),
        "listen" => Some(SourceType::Listen),
        "transform" => Some(SourceType::Transform),
        "workflow" => Some(SourceType::Workflow),
        _ => None,
    }
}

// ─── Entry commands ───────────────────────────────────────────────────────────

/// Fetch recent entries with optional source_type filter, limit and offset.
///
/// Exposed as `list_entries` for the frontend.
#[tauri::command(rename_all = "snake_case")]
pub fn list_entries(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
    source_type: Option<String>,
) -> Result<Vec<Entry>, String> {
    eprintln!("fonos: list_entries limit={:?} offset={:?} source_type={:?}", limit, offset, source_type);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let parsed = source_type.as_deref().and_then(parse_source_type);
    eprintln!("fonos: list_entries parsed_filter={:?}", parsed);
    let filter = EntryFilter {
        source_type: parsed,
        limit,
        offset,
        order_desc: true,
    };
    let result = storage::get_entries(&conn, &filter).map_err(|e| e.to_string())?;
    eprintln!("fonos: list_entries returned {} entries", result.len());
    Ok(result)
}

/// Fetch a single entry by its row ID.
#[tauri::command(rename_all = "snake_case")]
pub fn get_entry(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Entry, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::get_entry(&conn, id).map_err(|e| e.to_string())
}

/// Full-text search over all entries.
#[tauri::command(rename_all = "snake_case")]
pub fn search_entries(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<Entry>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::search_entries(&conn, &query, limit.unwrap_or(50))
        .map_err(|e| e.to_string())
}

/// Update the text of an existing entry (raw_text only; processed_text cleared).
#[tauri::command(rename_all = "snake_case")]
pub fn update_entry(
    state: tauri::State<'_, AppState>,
    id: i64,
    text: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::update_entry(&conn, id, &text, None).map_err(|e| e.to_string())
}

/// Update only an entry's processed (display) text, preserving the raw
/// transcript. Used by the History correction flow after a vocab rule/term is
/// saved.
#[tauri::command(rename_all = "snake_case")]
pub fn update_entry_text(
    state: tauri::State<'_, AppState>,
    id: i64,
    text: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::update_entry_processed_text(&conn, id, &text).map_err(|e| e.to_string())
}

/// Delete an entry by its row ID.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_entry(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    // Listen items own an audio file under the app data dir — remove it with
    // the row (only ever paths inside our own listen/ directory).
    let audio_ref = storage::get_entry(&conn, id)
        .ok()
        .and_then(|e| e.audio_ref);
    storage::delete_entry(&conn, id).map_err(|e| e.to_string())?;
    if let Some(path) = audio_ref {
        let listen_dir = fonos_core::config::AppConfig::config_dir().join("listen");
        if std::path::Path::new(&path).starts_with(&listen_dir) {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

// ─── Container commands ───────────────────────────────────────────────────────

/// Create a new notebook container with the given title.
///
/// Returns the newly created `Container` (with its assigned `id`).
#[tauri::command(rename_all = "snake_case")]
pub fn create_container(
    state: tauri::State<'_, AppState>,
    title: String,
    container_type: Option<String>,
) -> Result<Container, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;

    let ctype = match container_type.as_deref().unwrap_or("notebook") {
        "conversation" => ContainerType::Conversation,
        "section" => ContainerType::Section,
        "meeting_session" => ContainerType::MeetingSession,
        "journal" => ContainerType::Journal,
        "research" => ContainerType::Research,
        _ => ContainerType::Notebook,
    };

    let now = now_iso8601();
    let container = Container {
        id: None,
        container_type: ctype,
        title,
        parent_id: None,
        created_at: now.clone(),
        updated_at: now,
        metadata: serde_json::Value::Null,
    };

    let id = storage::insert_container(&conn, &container).map_err(|e| e.to_string())?;

    // Fetch and return the inserted container so the frontend gets the assigned ID
    storage::get_container(&conn, id).map_err(|e| e.to_string())
}

/// List all containers (notebooks, conversations, etc.).
#[tauri::command(rename_all = "snake_case")]
pub fn list_containers(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<Container>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::get_containers(&conn).map_err(|e| e.to_string())
}

/// Fetch entries belonging to a specific container (chronological order).
#[tauri::command(rename_all = "snake_case")]
pub fn get_container_entries(
    state: tauri::State<'_, AppState>,
    container_id: i64,
) -> Result<Vec<Entry>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::get_container_entries(&conn, container_id).map_err(|e| e.to_string())
}

/// Update a container's metadata JSON.
#[tauri::command(rename_all = "snake_case")]
pub fn update_container_metadata(
    state: tauri::State<'_, AppState>,
    id: i64,
    metadata: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE containers SET metadata = ?2, updated_at = ?3 WHERE id = ?1",
        rusqlite::params![id, metadata, now_iso8601()],
    ).map_err(|e| format!("update_container_metadata: {e}"))?;
    Ok(())
}

/// Delete a container by its row ID.
#[tauri::command(rename_all = "snake_case")]
pub fn delete_container(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    storage::delete_container(&conn, id).map_err(|e| e.to_string())
}

// ─── Export commands ───────────────────────────────────────────────────────────

/// Export a notebook as a Markdown folder.
///
/// Writes `README.md` (and optionally date-named `.md` files) inside a
/// subdirectory of `output_dir`, and returns the path to that directory.
#[tauri::command(rename_all = "snake_case")]
pub fn export_notebook_md(
    state: tauri::State<'_, AppState>,
    container_id: i64,
    output_dir: String,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let dir = PathBuf::from(&output_dir);
    let result_path = storage::export_notebook_markdown(&conn, container_id, &dir)
        .map_err(|e| e.to_string())?;
    Ok(result_path.to_string_lossy().into_owned())
}

/// Export a notebook as a JSON file.
///
/// Writes `{title}.json` into `output_dir` and returns the file path.
#[tauri::command(rename_all = "snake_case")]
pub fn export_notebook_json(
    state: tauri::State<'_, AppState>,
    container_id: i64,
    output_dir: String,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let dir = PathBuf::from(&output_dir);
    let result_path = storage::export_notebook_json(&conn, container_id, &dir)
        .map_err(|e| e.to_string())?;
    Ok(result_path.to_string_lossy().into_owned())
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Returns the current UTC time as an ISO 8601 string (YYYY-MM-DDTHH:MM:SS).
pub fn now_iso8601() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

pub fn days_to_ymd(mut days: i64) -> (i64, i64, i64) {
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
