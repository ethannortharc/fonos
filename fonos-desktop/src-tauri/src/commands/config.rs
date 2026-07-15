//! Tauri commands for reading and writing application configuration.

use super::AppState;

/// Return the current application configuration as a JSON object.
#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.config.lock().map_err(|e| e.to_string())?;
    let json = serde_json::to_value(&*guard).map_err(|e| e.to_string())?;
    Ok(json)
}

/// Whether dictation will actually transcribe with the live config — the single
/// runtime-backed STT gate. Delegates to the core resolver
/// ([`fonos_core::services::is_stt_effectively_configured`]) so the frontend
/// first-run gate and Apple-STT seed read the same rule the dictation pipeline
/// obeys, instead of re-deriving it in TypeScript.
#[tauri::command]
pub fn stt_configured(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let guard = state.config.lock().map_err(|e| e.to_string())?;
    Ok(fonos_core::services::is_stt_effectively_configured(&guard))
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

    // Unlock detection (onboarding P2): capture the role defaults before the
    // merge so the empty→non-empty transition is observable after it.
    let old_llm = guard.llm_profile.clone();
    let old_tts = guard.tts_profile.clone();

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
                || k == "pill_hotkey"
                || k == "pill_hotkey_capture"
        })
    });

    // Snapshot the small subset that satellite windows (the float pill) live-
    // consume in their own loadCfg, for the config:saved event below. Captured
    // before `updated` is moved into the shared state.
    let saved_ui_language = updated.ui_language.clone();
    let saved_active_voice_workflow = updated.active_voice_workflow.clone();
    let saved_llm = updated.llm_profile.clone();
    let saved_tts = updated.tts_profile.clone();

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

    // Onboarding P2: unlock notifications (once ever, funnel-gated) + tray
    // repaint. Every branch is best-effort — config saving must never fail
    // on notification/tray issues.
    {
        use fonos_core::workflow::builtin::resolve_lang;
        let lang = resolve_lang(&saved_ui_language);
        if crate::tray::unlocked(&old_llm, &saved_llm) {
            let newly = state
                .db
                .lock()
                .ok()
                .map(|db| fonos_core::funnel::record(&db, "llm_unlock_notified").unwrap_or(false))
                .unwrap_or(false);
            if newly {
                crate::tray::notify_unlock(&app, crate::tray::UnlockRole::Llm, lang);
            }
        }
        if crate::tray::unlocked(&old_tts, &saved_tts) {
            let newly = state
                .db
                .lock()
                .ok()
                .map(|db| fonos_core::funnel::record(&db, "tts_unlock_notified").unwrap_or(false))
                .unwrap_or(false);
            if newly {
                crate::tray::notify_unlock(&app, crate::tray::UnlockRole::Tts, lang);
            }
        }
        crate::tray::refresh_tray_status(&app, None);
    }

    Ok(())
}
