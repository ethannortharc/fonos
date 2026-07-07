//! Text actions — generic "grab selection → mode LLM step → deliver" pipeline.
//!
//! One orchestrator serves every configured `TextActionBinding`; delivery is
//! dispatched on the binding's `OutputTarget`. Every run is auto-logged to
//! history (SourceType::Transform); the popup's Save button links the entry
//! to the "Text Actions" notebook.

use fonos_core::config::TextActionBinding;
use fonos_core::modes::OutputTarget;

use super::AppState;

/// Run JS in the text-action panel webview (mirrors `agent_js` in main.rs).
/// Strings passed to recvXxx() are pre-escaped by callers via serde_json.
///
/// Security note: `WebviewWindow::eval` injects JS into an app-owned Tauri
/// webview (the bundled `text-action-panel.html`), not a general code-exec
/// sink for untrusted/external input — same pattern as `agent_js` in
/// main.rs. Every value interpolated into `js` here is escaped via
/// `serde_json::to_string` by the caller before reaching this function.
fn panel_js(h: &tauri::AppHandle, js: &str) {
    use tauri::Manager;
    if let Some(panel) = h.get_webview_window("text-action-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: text-action panel JS: {e}");
        }
    }
}

/// Show the panel near the cursor and focus it (focus enables Esc/blur dismissal).
async fn show_panel_at_cursor(handle: &tauri::AppHandle) {
    use tauri::Manager;
    // NOTE: `move_text_action_panel_to_cursor` lives in `commands/mod.rs`,
    // not `crate::` (main.rs) as the task brief assumed — see the doc
    // comment on `commands::monitor_under_cursor` for why (main.rs and
    // lib.rs each declare their own independent `mod commands;`, and a
    // crate-root item defined only in main.rs isn't visible when this file
    // is compiled as part of the lib.rs crate root).
    #[cfg(target_os = "macos")]
    super::move_text_action_panel_to_cursor(handle);
    if let Some(panel) = handle.get_webview_window("text-action-panel") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    // Let the webview settle before eval() — same trick as the agent panel.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
}

/// Route an error to the right surface: panel error state for popup targets,
/// float pill otherwise. Copy goes through the shared error classifier.
fn deliver_error(handle: &tauri::AppHandle, target: &OutputTarget, raw: &str) {
    if *target == OutputTarget::FloatingPopup {
        eprintln!("fonos: text action error: {raw}");
        let msg = crate::error_surface::classify_error(raw).message;
        let m_j = serde_json::to_string(&msg).unwrap_or_default();
        panel_js(handle, &format!("recvError({m_j})"));
    } else {
        crate::error_surface::emit_float_error(handle, raw);
    }
}

/// The generic text-action pipeline. See docs/fonos-text-actions-design.md §5.
pub async fn run_text_action(handle: tauri::AppHandle, binding: TextActionBinding) {
    use tauri::Manager;

    // 1. Grab the selection while the source app still has focus.
    let sel = match super::selection::grab_selection().await {
        Ok(s) if !s.text.is_empty() => s,
        _ => {
            show_panel_at_cursor(&handle).await;
            let m_j = serde_json::to_string("No text selected").unwrap_or_default();
            panel_js(&handle, &format!("recvHint({m_j})"));
            return;
        }
    };

    // 2. Resolve the mode.
    let all_modes = fonos_core::modes::all_modes();
    let Some(mode_def) = all_modes.get(&binding.mode_id).cloned() else {
        eprintln!("fonos: text action — mode '{}' not found", binding.mode_id);
        return;
    };

    // ActiveTextField can't paste into a read-only selection (PDF, web page
    // body) — fall back to the popup so the result isn't lost.
    let target = match binding.output_target {
        OutputTarget::ActiveTextField if !sel.editable => OutputTarget::FloatingPopup,
        ref t => t.clone(),
    };

    eprintln!(
        "fonos: text action mode={} target={:?} — {} chars from {}",
        binding.mode_id, target, sel.text.len(), sel.app_name
    );

    // 3. Popup targets show a thinking state before the LLM round-trip.
    if target == OutputTarget::FloatingPopup {
        show_panel_at_cursor(&handle).await;
        let icon_j = serde_json::to_string(&mode_def.icon).unwrap_or_default();
        let name_j = serde_json::to_string(&mode_def.name).unwrap_or_default();
        let preview: String = sel.text.chars().take(160).collect();
        let sel_j = serde_json::to_string(&preview).unwrap_or_default();
        panel_js(&handle, &format!("recvStart({icon_j}, {name_j}, {sel_j})"));
    }

    // 4. Resolve LLM service (mode override → global profile) and call it.
    let (translate_target, svc) = {
        let state: tauri::State<'_, AppState> = handle.state();
        let tt = state.config.lock().map(|c| c.translate_target.clone()).unwrap_or_default();
        let svc = if !mode_def.model.is_empty() {
            super::get_service_config_for_profile(&state, &mode_def.model)
        } else {
            super::get_service_config(&state, "llm")
        };
        (tt, svc)
    };
    let messages = fonos_core::llm::build_mode_messages(&mode_def, &sel.text, &translate_target);

    let result = match svc.provider.as_str() {
        "anthropic" => fonos_core::llm::call_anthropic(&svc.api_key, &svc.model, &messages, mode_def.temperature, mode_def.max_tokens).await,
        "google" => fonos_core::llm::call_google(&svc.api_key, &svc.model, &messages, mode_def.temperature, mode_def.max_tokens).await,
        _ => fonos_core::llm::call_openai_compatible(&svc.api_key, &svc.model, &svc.base_url, &messages, mode_def.temperature, mode_def.max_tokens, &svc.provider).await,
    };

    let text = match result {
        Ok(resp) if !resp.text.is_empty() => resp.text,
        Ok(_) => {
            deliver_error(&handle, &target, "LLM returned an empty response");
            return;
        }
        Err(e) => {
            deliver_error(&handle, &target, &format!("{e}"));
            return;
        }
    };

    // 5. Auto-log to history — every run, regardless of target. For the
    //    notebook target the entry is born linked to the notebook.
    let entry_id = {
        let state: tauri::State<'_, AppState> = handle.state();
        let db = match state.db.lock() {
            Ok(d) => d,
            Err(e) => {
                // deliver_error already logs the raw cause (popup: eprintln,
                // other targets: emit_float_error) — don't double-log here.
                deliver_error(&handle, &target, &format!("history db unavailable: {e}"));
                return;
            }
        };
        let container_id = if target == OutputTarget::AppendToContainer {
            ensure_text_actions_notebook(&db).ok()
        } else {
            None
        };
        let entry = fonos_core::storage::Entry {
            id: None,
            created_at: super::storage::now_iso8601(),
            source_type: fonos_core::storage::SourceType::Transform,
            role: fonos_core::storage::EntryRole::User,
            mode: binding.mode_id.clone(),
            raw_text: sel.text.clone(),
            processed_text: Some(text.clone()),
            container_id,
            audio_ref: None,
            metadata: serde_json::json!({
                "app_name": sel.app_name,
                "output_target": serde_json::to_value(&target).unwrap_or(serde_json::Value::Null),
                "provider": svc.provider,
                "model": svc.model,
            }),
        };
        match fonos_core::storage::insert_entry(&db, &entry) {
            Ok(id) => id,
            Err(e) => {
                // For AppendToContainer the DB insert *is* the delivery — a
                // silent eprintln would lose the result with no feedback.
                // Other targets already delivered (or are about to, below),
                // so keep the quiet fallback; the Save button guards
                // entry_id <= 0.
                if target == OutputTarget::AppendToContainer {
                    crate::error_surface::emit_float_error(
                        &handle,
                        &format!("could not save to notebook: {e}"),
                    );
                } else {
                    eprintln!("fonos: text action — insert entry: {e}");
                }
                0
            }
        }
    };

    // 6. Deliver.
    match target {
        OutputTarget::FloatingPopup => {
            let text_j = serde_json::to_string(&text).unwrap_or_default();
            let app_j = serde_json::to_string(&sel.app_name).unwrap_or_default();
            panel_js(&handle, &format!(
                "recvResult({text_j}, {entry_id}, {app_j}, {})", sel.editable
            ));
        }
        OutputTarget::ActiveTextField => {
            if let Err(e) = super::selection::replace_selection(text, Some(sel.app_name)).await {
                crate::error_surface::emit_float_error(&handle, &e);
            }
        }
        OutputTarget::Clipboard => {
            if let Err(e) = text_action_copy(text) {
                crate::error_surface::emit_float_error(&handle, &e);
            }
        }
        // AppendToContainer already persisted with container_id above; None is done too.
        OutputTarget::AppendToContainer | OutputTarget::None => {}
    }
}

/// Find-or-create the fixed "Text Actions" notebook (lazy, by title —
/// same pattern as the "Quick Note" lookup in commands/mod.rs).
fn ensure_text_actions_notebook(db: &rusqlite::Connection) -> Result<i64, String> {
    if let Ok(id) = db.query_row(
        "SELECT id FROM containers WHERE container_type='notebook' AND title='Text Actions' LIMIT 1",
        [], |r| r.get::<_, i64>(0),
    ) {
        return Ok(id);
    }
    let now = super::storage::now_iso8601();
    fonos_core::storage::insert_container(db, &fonos_core::storage::Container {
        id: None,
        container_type: fonos_core::storage::ContainerType::Notebook,
        title: "Text Actions".into(),
        parent_id: None,
        created_at: now.clone(),
        updated_at: now,
        metadata: serde_json::json!({}),
    }).map_err(|e| e.to_string())
}

// ─── Tauri commands (called from text-action-panel.html) ─────────────────────

#[tauri::command(rename_all = "snake_case")]
pub fn hide_text_action_panel(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("text-action-panel") {
        let _ = w.hide();
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn text_action_copy(text: String) -> Result<(), String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| format!("clipboard error: {e}"))?;
    cb.set_text(&text).map_err(|e| format!("clipboard set error: {e}"))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn text_action_insert(
    app: tauri::AppHandle,
    text: String,
    target_app: Option<String>,
) -> Result<(), String> {
    // Hide first so focus can return to the source app before Cmd+V.
    let _ = hide_text_action_panel(app.clone());
    if let Err(e) = super::selection::replace_selection(text, target_app).await {
        // The panel is already gone (blur-hide on focus switch), so surface
        // the failure on the float pill instead.
        crate::error_surface::emit_float_error(&app, &e);
        return Err(e);
    }
    Ok(())
}

#[tauri::command(rename_all = "snake_case")]
pub fn text_action_save_notebook(
    state: tauri::State<'_, AppState>,
    entry_id: i64,
) -> Result<i64, String> {
    if entry_id <= 0 {
        return Err("entry was not saved to history".into());
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    let nb_id = ensure_text_actions_notebook(&db)?;
    fonos_core::storage::update_entry_container(&db, entry_id, Some(nb_id))
        .map_err(|e| e.to_string())?;
    Ok(nb_id)
}
