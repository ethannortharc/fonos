//! Session-type Meeting output: continuous meeting capture + AI summary,
//! folding the legacy `hotkey_meeting` toggle arm's start/stop dance into one
//! composite output — reusing the exact `recv*` JS contract the legacy arm
//! drove (see `main.rs`'s former `"meeting"` dispatch arm, retired by
//! Workbench P2 Task 7), plus a real `recvMeetingError` handler where the
//! legacy arm's eval was dead (see the module docs on that below).
//!
//! [`MeetingOutput`] is the terminal [`Output`] a `meeting` widget resolves
//! to. Unlike `agent`/`dialog`, it has no empty-vs-non-empty input branch —
//! `wf.meeting` is always `src.instant`-sourced (always empty text) and the
//! composite reads the live [`crate::commands::meeting::MeetingState::recording`]
//! flag itself to decide which half of the toggle to run, exactly as the
//! legacy arm's key-down handler did.
//!
//! Ref resolution (Task 4's template, copied twice — once per capability):
//! [`MeetingProps::stt_widget`] wins when non-empty (its `stt` widget's
//! `model_profile` resolved via `get_service_config_for_profile`); empty
//! falls back to the legacy `meeting_stt_profile` config field, then the
//! global `"stt"` profile. **This is BUG 1's fix**: `commands::meeting`'s old
//! `start_meeting` used to read the global `"stt"` profile unconditionally,
//! silently ignoring `meeting_stt_profile` entirely — see
//! [`resolve_meeting_stt_widget_ref`] and [`MeetingOutput::start`].
//! [`MeetingProps::llm_widget`] resolves the same way, falling back to the
//! pre-existing `meeting_llm_profile`→`llm_profile` chain (unchanged from
//! `commands::meeting::call_llm_raw`'s old inline behavior) — see
//! [`resolve_meeting_llm_widget_ref`] and [`MeetingOutput::stop`].
//!
//! Both legacy profile fields stay live-read (not one-time-migrated) because
//! they name model profiles, not widgets — there is no ref they can become.
//! `MeetingProps::summary_prompt`, by contrast, is free text, so it — and
//! only it — was one-time-migrated from `config.meeting_summary_prompt` by
//! `migrate_legacy_meeting_triggers`; that field is truly DEPRECATED (see its
//! doc comment) and is never read at resolve time here. Its resolution is
//! simply: prop non-empty wins, else `commands::meeting::build_summary_prompt`'s
//! built-in literal.
//!
//! Dead-eval fix: the legacy arm's start branch called
//! `panel.eval("recvMeetingShow()")` right after showing the panel —
//! `meeting-panel.html` never defined `recvMeetingShow`, so that call was a
//! silent no-op (same shape of bug as the agent panel's pre-Task-6
//! `recvSkillExec`/etc. calls). [`MeetingOutput::start`] replaces it with a
//! real `recvStart(title, "")` eval (immediately resetting the panel's
//! transcript/title, before the spawned capture loop's own later, more
//! accurate `recvStart(title, channel_mode)` call lands) — seeing why this
//! needs the pre-generated `title` from `commands::meeting::default_meeting_title`
//! rather than one invented on the spot, see that function's doc comment.
//! The legacy arm's capture-init-failure path already called
//! `recvMeetingError(...)` (also previously dead — the panel had no such
//! function either); rather than reroute those calls through an existing
//! handler, `meeting-panel.html` gained a real minimal `recvMeetingError`
//! (display the message, stop the timer) — chosen over rerouting because an
//! init failure is a genuinely different state from "stopped, summarizing"
//! (`recvStop`) or "done" (`recvSummary`), and conflating them would either
//! misrepresent the error as a normal stop or require the panel to guess
//! which case an empty/error string means.
//!
//! Lock discipline: every `state.config.lock()` use here is a tight scope
//! (widget-ref resolution is synchronous) dropped before the next `.await` —
//! same discipline as `agent_widget.rs`/`dialog.rs`/`workflow_widgets.rs`.

use serde::{Deserialize, Serialize};
use tauri::Manager;

use fonos_core::workflow::model::{Data, DataKind, WidgetDef};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::meeting::{self, default_meeting_title, js_escape, meeting_js};
use super::AppState;

// ─── MeetingProps ───────────────────────────────────────────────────────────────

/// Configuration for a `meeting`-type composite widget's output — the
/// per-recipe knobs layered on top of `commands::meeting::MeetingState`'s
/// process-global runtime (recording flag / container id / chunk counter stay
/// global state, shared by every meeting entry point — this composite AND
/// the panel's own `stop_meeting` invocation alike).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeetingProps {
    /// Id of a tuned `stt` widget this meeting should resolve its
    /// transcription model profile from instead of the legacy
    /// `meeting_stt_profile`→global-`"stt"` fallback chain (Workbench P2
    /// Task 7, additive — mirrors `DialogProps`/`AgentProps::llm_widget`; see
    /// [`resolve_meeting_stt_widget_ref`]). **BUG 1 fix**: this is the ref
    /// half of it — the fallback chain itself now also honors
    /// `meeting_stt_profile`, which the pre-Task-7 code silently ignored.
    #[serde(default)]
    pub stt_widget: String,
    /// Id of a tuned `llm` widget this meeting should resolve its summary
    /// model profile from instead of the `meeting_llm_profile`→`llm_profile`
    /// config fallback chain (same template, applied to the summary LLM
    /// call — see [`resolve_meeting_llm_widget_ref`]).
    #[serde(default)]
    pub llm_widget: String,
    /// Inline summary-generation instructions, replacing
    /// `commands::meeting::BUILTIN_SUMMARY_INSTRUCTIONS` when non-empty.
    /// Empty means the built-in literal (unchanged pre-Task-7 behavior).
    /// Seeded from the now-deprecated `config.meeting_summary_prompt` by
    /// `migrate_legacy_meeting_triggers` for upgrading users; a brand-new
    /// meeting widget starts with this blank.
    #[serde(default)]
    pub summary_prompt: String,
}

/// Resolve [`MeetingProps::stt_widget`] — Task 4's ref-resolution template
/// applied to the meeting composite (mirrors
/// `agent_widget::resolve_agent_llm_widget_ref`): a non-empty ref is looked
/// up in `widgets`, erroring on a dangling ref or a `type_tag` other than
/// `"stt"`; on success its `model_profile` is returned (`stt_prompt`/
/// `vocab_books`/`temperature`/`language` are irrelevant here — the meeting
/// capture loop always transcribes via its own fixed 10-second-chunk
/// pipeline in `commands::meeting`, not the generic `SttProcessor`). An empty
/// ref returns `Ok(None)`, telling the caller to fall back to the legacy
/// `meeting_stt_profile` config field, then the global `"stt"` profile — see
/// [`MeetingOutput::start`].
pub(crate) fn resolve_meeting_stt_widget_ref(
    props: &MeetingProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.stt_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.stt_widget)
        .ok_or_else(|| format!("meeting: stt_widget '{}' not found", props.stt_widget))?;
    if widget.type_tag != "stt" {
        return Err(format!(
            "meeting stt_widget '{}' is a '{}' widget, expected 'stt'",
            props.stt_widget, widget.type_tag
        ));
    }
    let stt_props: super::workflow_widgets::SttProps = serde_json::from_value(widget.props.clone())
        .map_err(|e| format!("meeting: stt_widget '{}' props: {e}", props.stt_widget))?;
    Ok(Some(stt_props.model_profile))
}

/// Resolve [`MeetingProps::llm_widget`] — same template as
/// [`resolve_meeting_stt_widget_ref`] applied to the `llm` capability: a
/// non-empty ref is looked up in `widgets`, erroring on a dangling ref or a
/// `type_tag` other than `"llm"`; on success its `model_profile` is returned
/// (`system`/`user_template` are irrelevant — the summary call is a single
/// raw-prompt completion, not a system+template exchange). An empty ref
/// returns `Ok(None)`, telling the caller to fall back to the pre-existing
/// `meeting_llm_profile`→`llm_profile` config chain — see
/// [`MeetingOutput::stop`].
pub(crate) fn resolve_meeting_llm_widget_ref(
    props: &MeetingProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.llm_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.llm_widget)
        .ok_or_else(|| format!("meeting: llm_widget '{}' not found", props.llm_widget))?;
    if widget.type_tag != "llm" {
        return Err(format!(
            "meeting llm_widget '{}' is a '{}' widget, expected 'llm'",
            props.llm_widget, widget.type_tag
        ));
    }
    let llm_props: fonos_core::workflow::llm_step::LlmProps = serde_json::from_value(widget.props.clone())
        .map_err(|e| format!("meeting: llm_widget '{}' props: {e}", props.llm_widget))?;
    Ok(Some(llm_props.model_profile))
}

// ─── MeetingOutput ──────────────────────────────────────────────────────────────

/// `meeting`: continuous meeting capture + AI summary, toggled by the live
/// `MeetingState::recording` flag (start when not recording, stop when
/// recording) — the composite reading of the legacy hotkey arm's key-down
/// toggle dance.
pub struct MeetingOutput {
    /// Handle used to reach `AppState` and the panel window.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: MeetingProps,
}

#[async_trait::async_trait]
impl Output for MeetingOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        match result {
            Data::Text(_) => {}
            Data::Audio(_) => return Err("meeting output expected text, got audio".to_string()),
        }

        let state: tauri::State<'_, AppState> = self.app.state();
        let is_recording = state.meeting.lock().await.recording;

        if !is_recording {
            self.start(&state).await
        } else {
            self.stop(&state).await
        }
    }
}

impl MeetingOutput {
    /// Not-recording half of the toggle: resolve the STT service, position +
    /// reveal the panel, start capture, and reset the panel's transcript via
    /// an immediate `recvStart` (replacing the legacy arm's dead
    /// `recvMeetingShow` eval — see the module docs).
    async fn start(&self, state: &tauri::State<'_, AppState>) -> Result<(), String> {
        // STT resolution: stt_widget ref → its model_profile (BUG 1 fix's ref
        // half) → the legacy meeting_stt_profile field (BUG 1 fix's fallback
        // half — the old code never read this at all) → the global "stt"
        // profile. One scoped lock, resolved synchronously, dropped before
        // any await below.
        let stt_svc = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let widgets = fonos_core::workflow::engine::effective_widgets(&config);
            match resolve_meeting_stt_widget_ref(&self.props, &widgets)? {
                Some(profile_id) => fonos_core::services::resolve_profile(&config, &profile_id),
                None if !config.meeting_stt_profile.is_empty() => {
                    fonos_core::services::resolve_profile(&config, &config.meeting_stt_profile)
                }
                None => fonos_core::services::resolve_service(&config, "stt"),
            }
        };

        super::move_meeting_panel_to_cursor(&self.app);
        if let Some(panel) = self.app.get_webview_window("meeting-panel") {
            let _ = panel.show();
            let _ = panel.set_focus();
        }
        // Let the webview settle before eval() — same trick as the agent/
        // dialog panels.
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;

        let title = default_meeting_title();
        match meeting::start_meeting_with(&self.app, state, stt_svc, title.clone()).await {
            Ok(_container_id) => {
                let title_j = serde_json::to_string(&title).unwrap_or_default();
                meeting_js(&self.app, &format!("recvStart({title_j}, '')"));
                Ok(())
            }
            Err(e) => {
                meeting_js(&self.app, &format!("recvMeetingError('{}')", js_escape(&e)));
                Err(e)
            }
        }
    }

    /// Recording half of the toggle: resolve the summary LLM profile and
    /// stop capture (delegates entirely to
    /// `commands::meeting::stop_meeting_with`, which also generates the
    /// summary, records it, notifies the panel, and schedules the delayed
    /// hide).
    async fn stop(&self, state: &tauri::State<'_, AppState>) -> Result<(), String> {
        // LLM resolution: llm_widget ref → its model_profile → the legacy
        // meeting_llm_profile→llm_profile chain (unchanged from the pre-Task-7
        // inline behavior). One scoped lock, dropped before any await below.
        let llm_profile_id = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let widgets = fonos_core::workflow::engine::effective_widgets(&config);
            match resolve_meeting_llm_widget_ref(&self.props, &widgets)? {
                Some(profile_id) => profile_id,
                None if !config.meeting_llm_profile.is_empty() => config.meeting_llm_profile.clone(),
                None => config.llm_profile.clone(),
            }
        };

        meeting::stop_meeting_with(&self.app, state, llm_profile_id, self.props.summary_prompt.clone())
            .await
            .map(|_summary| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::workflow::model::WidgetRole;

    fn base_props() -> MeetingProps {
        serde_json::from_value(serde_json::json!({})).unwrap()
    }

    fn stt_widget_def(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "stt".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({
                "model_profile": model_profile,
                "stt_prompt": "",
                "vocab_books": [],
                "temperature": 0.0,
                "language": "auto",
            }),
            builtin: false,
        }
    }

    fn llm_widget_def(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "model_profile": model_profile, "system": null }),
            builtin: false,
        }
    }

    // ── MeetingProps serde defaults ──────────────────────────────────────────

    #[test]
    fn meeting_props_defaults_from_empty_json() {
        let props: MeetingProps = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(props.stt_widget, "");
        assert_eq!(props.llm_widget, "");
        assert_eq!(props.summary_prompt, "");
    }

    #[test]
    fn meeting_props_old_json_without_new_fields_still_parses() {
        // Back-compat: a persisted meeting widget missing any of these keys
        // still deserializes.
        let json = r#"{"stt_widget": "stt.tuned"}"#;
        let props: MeetingProps = serde_json::from_str(json).unwrap();
        assert_eq!(props.stt_widget, "stt.tuned");
        assert_eq!(props.llm_widget, "");
        assert_eq!(props.summary_prompt, "");
    }

    #[test]
    fn meeting_props_roundtrip_preserves_custom_values() {
        let props = MeetingProps {
            stt_widget: "stt.tuned".into(),
            llm_widget: "llm.tuned".into(),
            summary_prompt: "Focus on decisions.".into(),
        };
        let json = serde_json::to_value(&props).unwrap();
        let back: MeetingProps = serde_json::from_value(json).unwrap();
        assert_eq!(back, props);
    }

    // ── resolve_meeting_stt_widget_ref ────────────────────────────────────────

    #[test]
    fn resolve_meeting_stt_widget_ref_empty_returns_none() {
        let props = base_props();
        assert_eq!(resolve_meeting_stt_widget_ref(&props, &[]).unwrap(), None);
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.stt_widget = "stt.tuned".into();
        let widgets = vec![stt_widget_def("stt.tuned", "tuned-stt-profile")];
        assert_eq!(
            resolve_meeting_stt_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-stt-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.stt_widget = "stt.missing".into();
        let err = resolve_meeting_stt_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("stt.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_meeting_stt_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.stt_widget = "llm.x".into();
        let wrong_type_widget = llm_widget_def("llm.x", "p");
        let err = resolve_meeting_stt_widget_ref(&props, &[wrong_type_widget]).unwrap_err();
        assert!(err.contains("expected 'stt'"), "error should mention type mismatch, got: {err}");
    }

    // ── resolve_meeting_llm_widget_ref ────────────────────────────────────────

    #[test]
    fn resolve_meeting_llm_widget_ref_empty_returns_none() {
        let props = base_props();
        assert_eq!(resolve_meeting_llm_widget_ref(&props, &[]).unwrap(), None);
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.llm_widget = "llm.tuned".into();
        let widgets = vec![llm_widget_def("llm.tuned", "tuned-llm-profile")];
        assert_eq!(
            resolve_meeting_llm_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-llm-profile".to_string())
        );
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.llm_widget = "llm.missing".into();
        let err = resolve_meeting_llm_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("llm.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_meeting_llm_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.llm_widget = "stt.x".into();
        let wrong_type_widget = stt_widget_def("stt.x", "p");
        let err = resolve_meeting_llm_widget_ref(&props, &[wrong_type_widget]).unwrap_err();
        assert!(err.contains("expected 'llm'"), "error should mention type mismatch, got: {err}");
    }
}
