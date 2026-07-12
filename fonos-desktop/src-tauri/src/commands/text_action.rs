//! Text actions — generic "grab selection → mode LLM step → deliver" pipeline.
//!
//! One orchestrator serves every configured `TextActionBinding`; delivery is
//! dispatched on the binding's `OutputTarget`. Every run is auto-logged to
//! history (SourceType::Transform); the popup's Save button links the entry
//! to the "Text Actions" notebook.

use super::AppState;

/// Run JS in the text-action panel webview (mirrors `agent_js` in main.rs).
/// Strings passed to recvXxx() are pre-escaped by callers via serde_json.
///
/// Security note: `WebviewWindow::eval` injects JS into an app-owned Tauri
/// webview (the bundled `text-action-panel.html`), not a general code-exec
/// sink for untrusted/external input — same pattern as `agent_js` in
/// main.rs. Every value interpolated into `js` here is escaped via
/// `serde_json::to_string` by the caller before reaching this function.
///
/// `pub(crate)` so the workflow `panel` output
/// ([`super::workflow_widgets`]) can reuse the exact recv protocol rather than
/// copying it.
pub(crate) fn panel_js(h: &tauri::AppHandle, js: &str) {
    use tauri::Manager;
    if let Some(panel) = h.get_webview_window("text-action-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: text-action panel JS: {e}");
        }
    }
}

/// Size the panel to `(w, h)`, position it near the cursor, and focus it (focus
/// enables Esc/blur dismissal).
///
/// `pub(crate)` so the workflow `panel` output ([`super::workflow_widgets`])
/// can size, position, and reveal the shared panel identically (the panel's size
/// comes from its `PanelSize` prop, mirroring [`super::dialog::show_dialog_at_cursor`]).
pub(crate) async fn show_panel_at_cursor(handle: &tauri::AppHandle, w: u32, h: u32) {
    use tauri::Manager;
    if let Some(panel) = handle.get_webview_window("text-action-panel") {
        let _ = panel.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            w as f64, h as f64,
        )));
    }
    // NOTE: `move_panel_to_cursor` lives in `commands/mod.rs`,
    // not `crate::` (main.rs) as the task brief assumed — see the doc
    // comment on `commands::monitor_under_cursor` for why (main.rs and
    // lib.rs each declare their own independent `mod commands;`, and a
    // crate-root item defined only in main.rs isn't visible when this file
    // is compiled as part of the lib.rs crate root).
    #[cfg(target_os = "macos")]
    super::move_panel_to_cursor(handle, "text-action-panel", w, h, super::PanelAnchor::Cursor);
    if let Some(panel) = handle.get_webview_window("text-action-panel") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    // Let the webview settle before eval() — same trick as the agent panel.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
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
