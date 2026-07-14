//! Onboarding funnel commands — local-only first-experience milestones.
//! Steps are recorded at most once (UNIQUE + INSERT OR IGNORE in core).

use super::AppState;

/// Record a funnel milestone once. Returns true when this call recorded it.
#[tauri::command]
pub fn record_onboarding_event(
    state: tauri::State<'_, AppState>,
    step: String,
) -> Result<bool, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    fonos_core::funnel::record(&conn, &step).map_err(|e| e.to_string())
}

/// All recorded funnel milestones, oldest first.
#[tauri::command]
pub fn get_onboarding_events(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<fonos_core::funnel::FunnelEvent>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    fonos_core::funnel::get_all(&conn).map_err(|e| e.to_string())
}
