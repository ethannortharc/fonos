//! Session-type Dialog output: opens the floating `dialog-panel` window with a
//! workflow's first turn, then drives live follow-up chat turns.
//!
//! [`DialogOutput`] is the terminal [`Output`] a `dialog` widget resolves to. On
//! delivery it materializes the run into a `Conversation` container (first user
//! + assistant turns), seeds a live [`DialogSession`], stashes it as the single
//! [`super::AppState::dialog_session`], and reveals the panel populated with the
//! two seeded bubbles. Follow-up turns come back through [`dialog_send`].
//!
//! Event single-point: the engine emits the terminal float-pill event after all
//! outputs succeed — this output only opens the window and never emits a pill.
//!
//! Lock discipline: every rusqlite call happens inside a scoped
//! `state.db.lock()` block that is dropped **before** any `.await`
//! (window / eval / `next_turn`). The `dialog_session` `tokio::sync::Mutex`
//! MAY be held across `next_turn().await` — that is why it is a tokio mutex.

use tauri::Manager;

use fonos_core::storage::{Container, ContainerType, Entry, EntryRole, SourceType};
use fonos_core::workflow::dialog::{DialogProps, DialogSession};
use fonos_core::workflow::model::{Data, DataKind};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::AppState;

/// Follow-up sampling temperature. `DialogEngine::Llm` carries no
/// temperature/max_tokens, so a conversational default is used for every
/// follow-up turn (chosen value — flagged for review).
const DIALOG_TEMPERATURE: f64 = 0.7;
/// Follow-up reply cap (chosen value — flagged for review).
const DIALOG_MAX_TOKENS: u32 = 2048;
/// How many user/assistant exchange pairs a Dialog session retains before the
/// rolling context trims the oldest (see [`DialogSession::new`]).
const DIALOG_MAX_TURNS: usize = 12;

/// The live desktop state behind an open Dialog panel. Wraps the core
/// [`DialogSession`] with the bits follow-up turns need but the core session
/// does not hold: which model profile the LLM service is re-resolved from.
pub struct ActiveDialog {
    /// Core rolling-history session (container id + system prompt + context).
    pub session: DialogSession,
    /// Model profile id the follow-up service is resolved from each turn
    /// (empty ⇒ the global `"llm"` profile).
    pub model_profile: String,
    /// Whether replies render as Markdown. The panel itself holds the live
    /// render flag (set once at `recvInit`), so follow-up turns don't re-send
    /// it; retained here for parity with the props and future engines.
    #[allow(dead_code)]
    pub markdown: bool,
}

/// Run JS in the dialog-panel webview (mirrors [`super::text_action::panel_js`]).
///
/// Security note: `WebviewWindow::eval` injects JS into an app-owned Tauri
/// webview (the bundled `dialog-panel.html`), not a general code-exec sink for
/// untrusted input. Every value interpolated into `js` is pre-escaped by
/// callers via `serde_json::to_string` before reaching this function.
pub(crate) fn dialog_js(h: &tauri::AppHandle, js: &str) {
    if let Some(panel) = h.get_webview_window("dialog-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: dialog panel JS: {e}");
        }
    }
}

/// Size the dialog panel to `(w, h)`, position it near the cursor, reveal +
/// focus it, then let the webview settle before `eval()` (mirrors
/// [`super::text_action::show_panel_at_cursor`], parameterized on size).
pub(crate) async fn show_dialog_at_cursor(handle: &tauri::AppHandle, w: u32, h: u32) {
    if let Some(panel) = handle.get_webview_window("dialog-panel") {
        let _ = panel.set_size(tauri::Size::Logical(tauri::LogicalSize::new(
            w as f64, h as f64,
        )));
    }
    // `move_panel_to_cursor` lives in `commands/mod.rs` (reachable via
    // `super::`) for the same lib.rs/main.rs module-split reason documented on
    // `commands::monitor_under_cursor`.
    #[cfg(target_os = "macos")]
    super::move_panel_to_cursor(handle, "dialog-panel", w, h, super::PanelAnchor::Cursor);
    if let Some(panel) = handle.get_webview_window("dialog-panel") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    // Let the webview settle before eval() — same trick as the agent/text panels.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
}

/// `dialog`: open the floating chat panel with the run's first turn and arm a
/// live follow-up session.
pub struct DialogOutput {
    /// Handle used to reach `AppState` (db + service resolution + session slot)
    /// and the panel window.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration (render mode, window size, engine).
    pub props: DialogProps,
}

#[async_trait::async_trait]
impl Output for DialogOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, ctx: &RunCtx) -> Result<(), String> {
        // Resolve which (model_profile, system) pair drives follow-up turns.
        // Workbench P2 Task 4 (additive): `self.props.llm_widget` wins when
        // non-empty — resolved against the effective widget set inside a
        // scoped config lock, dropped before any await below (the registry
        // factory that built `self.props` only ever receives raw props, not
        // the widget list, so this can't be resolved any earlier than here).
        // Empty `llm_widget` falls back to the inline `engine` fields exactly
        // as before Task 4; P2's non-`Llm` engine placeholders still error
        // when there's no ref to fall back on instead.
        let (model_profile, system) = {
            let state = self.app.state::<AppState>();
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let widgets = fonos_core::workflow::engine::effective_widgets(&config);
            fonos_core::workflow::dialog::resolve_llm_engine(&self.props, &widgets)?
        };

        let final_text = match result {
            Data::Text(t) => t.clone(),
            Data::Audio(_) => return Err("dialog output expected text, got audio".to_string()),
        };

        // The engine wrote the run's history entry id into meta before delivery
        // (CRUX 1). Read it in a tight scope — meta is a std Mutex.
        let entry_id = ctx
            .meta
            .lock()
            .ok()
            .and_then(|m| m.get("entry_id").and_then(|v| v.as_i64()))
            .unwrap_or(0);

        // Read the raw source text + workflow id/name off the recorded entry,
        // in its own scoped db-lock block (dropped before the config lock
        // below is taken — never held simultaneously, so this can't invert
        // lock order against any other call site).
        //
        // The recorder's entry has raw_text = the original source text and
        // metadata.workflow_id/workflow_name for the title. entry_id <= 0
        // (recorder failed / no id) ⇒ fall back to `final` for the user turn
        // and a plain "Dialog" title.
        let (raw_text, wf_id, stored_name) = {
            let state = self.app.state::<AppState>();
            let db = state.db.lock().map_err(|e| e.to_string())?;
            if entry_id > 0 {
                match fonos_core::storage::get_entry(&db, entry_id) {
                    Ok(e) => {
                        let stored_name = e
                            .metadata
                            .get("workflow_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Dialog")
                            .to_string();
                        let wf_id = e
                            .metadata
                            .get("workflow_id")
                            .and_then(|v| v.as_str())
                            .map(str::to_string);
                        (e.raw_text, wf_id, stored_name)
                    }
                    Err(_) => (final_text.clone(), None, "Dialog".to_string()),
                }
            } else {
                (final_text.clone(), None, "Dialog".to_string())
            }
        };

        // Workbench P2 Task 13: prefer localizing `workflow_id` through the
        // builtin display map AT EMISSION TIME (this panel's own
        // `config.ui_language`, resolved fresh here rather than trusting
        // whatever language the recorder happened to localize
        // `workflow_name` to) — falls back to the stored `workflow_name`
        // (already localized by `DbRecorder`, a custom recipe's own name, or
        // the "Dialog" default above) when the id is missing or not a
        // builtin.
        let wf_name = match &wf_id {
            Some(id) => {
                let state = self.app.state::<AppState>();
                let lang = match state.config.lock() {
                    Ok(config) => fonos_core::workflow::builtin::resolve_lang(&config.ui_language),
                    Err(_) => fonos_core::workflow::builtin::resolve_lang("auto"),
                };
                fonos_core::workflow::builtin::builtin_display_name(id, lang)
                    .map(str::to_string)
                    .unwrap_or(stored_name)
            }
            None => stored_name,
        };

        // All remaining rusqlite work in one scoped block, dropped before any
        // await: create the Conversation container and write the two seed
        // turns.
        let (raw_text, title, cid) = {
            let state = self.app.state::<AppState>();
            let db = state.db.lock().map_err(|e| e.to_string())?;

            let now = super::storage::now_iso8601();
            let title = format!("{wf_name} · {now}");
            let cid = fonos_core::storage::insert_container(
                &db,
                &Container {
                    id: None,
                    container_type: ContainerType::Conversation,
                    title: title.clone(),
                    parent_id: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    metadata: serde_json::json!({ "source": "dialog" }),
                },
            )
            .map_err(|e| e.to_string())?;

            // First user turn (original source text) + first assistant turn (final).
            let user_entry = Entry {
                id: None,
                created_at: now.clone(),
                source_type: SourceType::Workflow,
                role: EntryRole::User,
                mode: "dialog".to_string(),
                raw_text: raw_text.clone(),
                processed_text: None,
                container_id: Some(cid),
                audio_ref: None,
                metadata: serde_json::json!({}),
            };
            fonos_core::storage::insert_entry(&db, &user_entry).map_err(|e| e.to_string())?;
            let assistant_entry = Entry {
                id: None,
                created_at: now,
                source_type: SourceType::Workflow,
                role: EntryRole::Assistant,
                mode: "dialog".to_string(),
                raw_text: final_text.clone(),
                processed_text: None,
                container_id: Some(cid),
                audio_ref: None,
                metadata: serde_json::json!({}),
            };
            fonos_core::storage::insert_entry(&db, &assistant_entry).map_err(|e| e.to_string())?;

            (raw_text, title, cid)
        }; // db lock dropped here — safe to await below.

        // Seed the live session and store it as THE active dialog (replacing any
        // prior; the prior Conversation container stays in history). model_profile
        // is retained so dialog_send re-resolves the LLM service per turn — the
        // core DialogSession does not itself hold a service or profile.
        let mut session = DialogSession::new(cid, system, DIALOG_MAX_TURNS);
        session.seed_first_turn(&raw_text, &final_text);
        {
            let slot = self.app.state::<AppState>().dialog_session.clone();
            let mut guard = slot.lock().await;
            *guard = Some(ActiveDialog {
                session,
                model_profile,
                markdown: self.props.markdown,
            });
        }

        // Reveal the panel and populate it with the two seeded bubbles. Every
        // interpolated arg is pre-escaped via serde_json::to_string.
        show_dialog_at_cursor(&self.app, self.props.size.width, self.props.size.height).await;
        let title_j = serde_json::to_string(&title).unwrap_or_else(|_| "\"Dialog\"".to_string());
        let user_j = serde_json::to_string(&raw_text).unwrap_or_else(|_| "\"\"".to_string());
        let asst_j = serde_json::to_string(&final_text).unwrap_or_else(|_| "\"\"".to_string());
        dialog_js(
            &self.app,
            &format!(
                "recvInit({title_j}, {user_j}, {asst_j}, {})",
                self.props.markdown
            ),
        );
        Ok(())
    }
}

// ─── Tauri commands (called from dialog-panel.html) ──────────────────────────

/// Hide the dialog panel (Esc / close button). Unlike the text-action panel
/// there is no blur-hide — a Dialog stays open while the user switches apps.
#[tauri::command(rename_all = "snake_case")]
pub fn hide_dialog_panel(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("dialog-panel") {
        let _ = w.hide();
    }
    Ok(())
}

/// Run one follow-up turn: persist the user message, ask the engine for a
/// reply (a thinking indicator shows meanwhile), persist the reply, and push it
/// to the panel. All turns land in the session's `Conversation` container so the
/// exchange is one coherent thread in history.
#[tauri::command(rename_all = "snake_case")]
pub async fn dialog_send(app: tauri::AppHandle, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }

    // Hold the tokio dialog lock across the whole turn (incl. next_turn().await)
    // — that is why `dialog_session` is a tokio::sync::Mutex.
    let slot = app.state::<AppState>().dialog_session.clone();
    let mut guard = slot.lock().await;
    let active = guard.as_mut().ok_or("no active dialog session")?;
    let container_id = active.session.container_id;
    let model_profile = active.model_profile.clone();

    // Persist the user turn (scoped std db lock, dropped before any await).
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let user_entry = Entry {
            id: None,
            created_at: super::storage::now_iso8601(),
            source_type: SourceType::Workflow,
            role: EntryRole::User,
            mode: "dialog".to_string(),
            raw_text: text.clone(),
            processed_text: None,
            container_id: Some(container_id),
            audio_ref: None,
            metadata: serde_json::json!({}),
        };
        fonos_core::storage::insert_entry(&db, &user_entry).map_err(|e| e.to_string())?;
    }

    dialog_js(&app, "recvThinking()");

    // Resolve the LLM service (config lock taken + dropped inside these helpers;
    // no std lock is held across the next_turn await below).
    let service = {
        let state = app.state::<AppState>();
        if model_profile.is_empty() {
            super::get_service_config(&state, "llm")
        } else {
            super::get_service_config_for_profile(&state, &model_profile)
        }
    };

    // Run the follow-up turn; the tokio guard is held across this await.
    let reply = match active
        .session
        .next_turn(&text, &service, DIALOG_TEMPERATURE, DIALOG_MAX_TOKENS)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let e_j = serde_json::to_string(&e).unwrap_or_else(|_| "\"\"".to_string());
            dialog_js(&app, &format!("recvError({e_j})"));
            return Err(e);
        }
    };

    // Persist the assistant turn (scoped std db lock again).
    {
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|e| e.to_string())?;
        let assistant_entry = Entry {
            id: None,
            created_at: super::storage::now_iso8601(),
            source_type: SourceType::Workflow,
            role: EntryRole::Assistant,
            mode: "dialog".to_string(),
            raw_text: reply.clone(),
            processed_text: None,
            container_id: Some(container_id),
            audio_ref: None,
            metadata: serde_json::json!({}),
        };
        fonos_core::storage::insert_entry(&db, &assistant_entry).map_err(|e| e.to_string())?;
    }

    let reply_j = serde_json::to_string(&reply).unwrap_or_else(|_| "\"\"".to_string());
    dialog_js(&app, &format!("recvTurn(\"assistant\", {reply_j})"));
    Ok(())
}

/// "Save to notebook" for a Dialog: its turns already live in a persisted
/// `Conversation` container, so this validates the id exists and hands it back
/// for the UI to link to / navigate to. Kept intentionally light per the brief.
#[tauri::command(rename_all = "snake_case")]
pub fn dialog_save_notebook(
    state: tauri::State<'_, AppState>,
    container_id: i64,
) -> Result<i64, String> {
    if container_id <= 0 {
        return Err("dialog was not persisted (no container id)".to_string());
    }
    let db = state.db.lock().map_err(|e| e.to_string())?;
    fonos_core::storage::get_container(&db, container_id).map_err(|e| e.to_string())?;
    Ok(container_id)
}
