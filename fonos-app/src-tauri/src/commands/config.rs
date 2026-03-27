//! Tauri commands for reading and writing application configuration.

use super::AppState;

/// Return the current application configuration as a JSON object.
#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.config.lock().map_err(|e| e.to_string())?;
    let json = serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
    Ok(json)
}

/// Merge the provided JSON fields into the config and persist to disk.
///
/// Only keys present in `config_json` are updated; unrecognised keys are
/// ignored. The full updated config is returned.
#[tauri::command]
pub fn save_config(
    state: tauri::State<'_, AppState>,
    config_json: String,
) -> Result<(), String> {
    // Parse the incoming JSON.
    let updates: serde_json::Value =
        serde_json::from_str(&config_json).map_err(|e| format!("invalid JSON: {e}"))?;

    let mut guard = state.config.lock().map_err(|e| e.to_string())?;

    // Round-trip: serialize current config → merge updates → deserialize back.
    let mut current =
        serde_json::to_value(&*guard).map_err(|e| format!("serialize error: {e}"))?;

    if let (Some(cur_obj), Some(upd_obj)) = (current.as_object_mut(), updates.as_object()) {
        for (k, v) in upd_obj {
            cur_obj.insert(k.clone(), v.clone());
        }
    }

    let updated: fonos_core::config::AppConfig =
        serde_json::from_value(current).map_err(|e| format!("deserialize error: {e}"))?;

    // Persist to disk.
    updated
        .save()
        .map_err(|e| format!("failed to save config: {e}"))?;


    // Update in-memory state.
    *guard = updated;

    Ok(())
}
