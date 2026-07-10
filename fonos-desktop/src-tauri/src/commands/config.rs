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
    app: tauri::AppHandle,
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


    // Check if any hotkey-related fields changed. `text_actions` itself
    // carries per-binding hotkeys and must trigger a re-register too, or
    // settings edits (new bindings, deletions, reordering) never take
    // effect until restart — deleted rows would misroute against stale
    // `text-action-{i}` labels resolved from the new array in the meantime.
    let hotkey_changed = updates.as_object().map_or(false, |u| {
        u.keys().any(|k| {
            k.starts_with("hotkey_")
                || k == "text_actions"
                || k.starts_with("notebook_hotkey_")
                || k == "workflows"
        })
    });

    // Snapshot the small subset that satellite windows (the float pill) live-
    // consume in their own loadCfg, for the config:saved event below. Captured
    // before `updated` is moved into the shared state.
    let saved_ui_language = updated.ui_language.clone();
    let saved_active_voice_workflow = updated.active_voice_workflow.clone();

    // Update in-memory state.
    *guard = updated;
    drop(guard); // release lock before emitting

    use tauri::Emitter;

    // If hotkeys changed, notify the hotkey manager to re-register.
    if hotkey_changed {
        eprintln!("fonos: hotkey config changed — emitting reload signal");
        let _ = app.emit("hotkey:reload", ());
    }

    // Notify satellite windows (e.g. the float pill) that config was saved so
    // they can live-update — the pill re-reads its UI language and active voice
    // workflow without a restart. Only the fields those windows read in their
    // own loadCfg are sent, not the whole config. This is a settings-change
    // signal, distinct from the engine's float:* terminal events.
    let _ = app.emit(
        "config:saved",
        serde_json::json!({
            "ui_language": saved_ui_language,
            "active_voice_workflow": saved_active_voice_workflow,
        }),
    );

    Ok(())
}
