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
use fonos_core::workflow::dialog::{DialogEngine, DialogProps, DialogSession};
use fonos_core::workflow::model::{Data, DataKind};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::AppState;

/// How many user/assistant exchange pairs a Dialog session retains before the
/// rolling context trims the oldest (see [`DialogSession::new`]).
const DIALOG_MAX_TURNS: usize = 12;

/// The live desktop state behind an open Dialog panel. Wraps the core
/// [`DialogSession`] with the bits follow-up turns need but the core session
/// does not hold: which model profile the LLM service is re-resolved from.
///
/// `session` / `model_profile` are consumed by `dialog_send` (added in the
/// follow-up commit); `markdown` mirrors the panel's render flag. The
/// struct-level allow is narrowed to just `markdown` once `dialog_send` lands.
#[allow(dead_code)]
pub struct ActiveDialog {
    /// Core rolling-history session (container id + system prompt + context).
    pub session: DialogSession,
    /// Model profile id the follow-up service is resolved from each turn
    /// (empty ⇒ the global `"llm"` profile).
    pub model_profile: String,
    /// Whether replies render as Markdown. The panel itself holds the live
    /// render flag (set once at `recvInit`), so follow-up turns don't re-send
    /// it; retained here for parity with the props and future engines.
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
    // `move_dialog_panel_to_cursor` lives in `commands/mod.rs` (reachable via
    // `super::`) for the same lib.rs/main.rs module-split reason documented on
    // `commands::monitor_under_cursor`.
    #[cfg(target_os = "macos")]
    super::move_dialog_panel_to_cursor(handle, w, h);
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
        // P2 only wires the plain-LLM engine; the placeholders open no window.
        let (model_profile, system) = match &self.props.engine {
            DialogEngine::Llm { model_profile, system } => (model_profile.clone(), system.clone()),
            DialogEngine::Agent {} | DialogEngine::Sts {} | DialogEngine::Workflow { .. } => {
                return Err("dialog engine not implemented in P2".to_string());
            }
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

        // All rusqlite work in one scoped block, dropped before any await:
        // read the raw source text + workflow name off the recorded entry,
        // create the Conversation container, and write the two seed turns.
        let (raw_text, title, cid) = {
            let state = self.app.state::<AppState>();
            let db = state.db.lock().map_err(|e| e.to_string())?;

            // The recorder's entry has raw_text = the original source text and
            // metadata.workflow_name for the title. entry_id <= 0 (recorder
            // failed / no id) ⇒ fall back to `final` for the user turn and a
            // plain title.
            let (raw_text, wf_name) = if entry_id > 0 {
                match fonos_core::storage::get_entry(&db, entry_id) {
                    Ok(e) => {
                        let name = e
                            .metadata
                            .get("workflow_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Dialog")
                            .to_string();
                        (e.raw_text, name)
                    }
                    Err(_) => (final_text.clone(), "Dialog".to_string()),
                }
            } else {
                (final_text.clone(), "Dialog".to_string())
            };

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
        let user_j = serde_json::to_string(&raw_text).unwrap_or_default();
        let asst_j = serde_json::to_string(&final_text).unwrap_or_default();
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
