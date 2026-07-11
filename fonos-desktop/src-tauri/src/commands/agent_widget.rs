//! Session-type Agent output: opens the floating `agent-panel` window and,
//! for non-empty input, runs one skill-wielding exchange (STT-or-selection
//! text in, skill executions + LLM response out) — reusing the exact
//! `recv*` JS contract the legacy agent hotkey arm drove (see `main.rs`'s
//! former `"agent"`/`"agent-panel"` dispatch arms, retired by Workbench P2
//! Task 6).
//!
//! [`AgentOutput`] is the terminal [`Output`] an `agent` widget resolves to.
//! Empty input (the blank-open `wf.agent` recipe, sourced from `src.instant`)
//! just positions + reveals the panel in "persistent" mode, exactly like the
//! old `"agent-panel"` toggle arm's show branch — the user then drives it via
//! the panel's own mic button / typed input, which still calls the unchanged
//! Tauri commands in `commands::agent` directly. Non-empty input (the voice
//! recipe `wf.agent-voice`, sourced from `src.mic-hold` → `stt.default`) runs
//! one full exchange via [`run_agent_exchange`].
//!
//! LLM resolution: [`AgentProps::llm_widget`] — Task 4's ref-resolution
//! template, copied verbatim (resolved synchronously inside a scoped
//! config-lock block, dropped before any await; a dangling ref or a
//! type_tag mismatch is a hard `Err`) — wins when non-empty, taking only the
//! referenced `llm` widget's `model_profile` (unlike Dialog, which also takes
//! `system`: the agent's system prompt is the separate, global
//! `agent_system_prompt` config field, never sourced from a widget). Empty
//! `llm_widget` falls back to the pre-existing `agent_llm_profile` →
//! `llm_profile` config chain, unchanged from `commands::agent::agent_process`.
//!
//! Selection-prefix parity: the legacy arm grabbed the frontmost selection
//! BEFORE showing the panel (so showing/focusing the panel never raced the
//! grab — see its "Grab selected text BEFORE showing panel" comment).
//! [`run_agent_exchange`] preserves that ordering: it grabs the selection as
//! its very first step, before any panel show/focus call.
//!
//! Dropped from the legacy arm (deliberate, see Task 6 report): the
//! auto-replace-into-selection side effect (pasting the response back into
//! the app the selection came from). The task's preserved-behavior list
//! covers process → recvSkillExec → recvResponse → optional TTS plus the
//! selection-as-prompt-context prefix — not the paste-back, which is a
//! `replace`-output concern, not a session/chat composite's.
//!
//! Lock discipline: every `state.config.lock()` / `state.audio_playback.lock()`
//! use here is a tight scope dropped before the next await (same discipline
//! as `dialog.rs`/`workflow_widgets.rs`).

use serde::{Deserialize, Serialize};
use tauri::Manager;

use fonos_core::workflow::model::{Data, DataKind, WidgetDef};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::AppState;

// ─── AgentProps ────────────────────────────────────────────────────────────────

/// Configuration for an `agent`-type composite widget's output — the
/// per-recipe knobs layered on top of `commands::agent::AgentState`'s
/// process-global runtime (skill registry / conversation context / safety
/// filter stay global config, shared by every agent entry point — this
/// composite AND the legacy panel typing/mic commands alike).
///
/// Safety allow/blocklist and the agent's system prompt are deliberately NOT
/// here: they are security policy / persona, not per-instance behavior — see
/// the brief's "safety 列表留 config 全局" note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentProps {
    /// Id of a tuned `llm` widget this agent should resolve its model profile
    /// from instead of the global `agent_llm_profile`→`llm_profile` fallback
    /// chain (Workbench P2 Task 6, additive — mirrors `DialogProps::llm_widget`;
    /// see [`resolve_agent_llm_widget_ref`]).
    #[serde(default)]
    pub llm_widget: String,
    /// Speak the response aloud via TTS after delivering it (mirrors the
    /// legacy `agent_tts_enabled` config default: off).
    #[serde(default)]
    pub tts_enabled: bool,
    /// TTS profile id, or empty to fall back to the global `"tts"` profile
    /// (same convention as `workflow_widgets::SpeakOutput::voice_profile`).
    #[serde(default)]
    pub voice_profile: String,
    /// Voice identifier passed to the TTS backend (same convention as
    /// `SpeakOutput::voice`; `"default"` resolves via `tts::resolve_voice`).
    #[serde(default = "default_voice")]
    pub voice: String,
    /// Skill-execution timeout in seconds (mirrors `agent_timeout_secs`'s
    /// default of 30). Overrides the shared `AgentState.timeout_secs` for
    /// this call only — see `commands::agent::run_agent_processor`.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Conversation memory depth in turns (mirrors `agent_max_turns`'s
    /// default of 20). NOTE: the underlying `AgentState.context` is one
    /// process-global rolling history shared by every agent entry point, so
    /// this field is carried for parity/future use but does not currently
    /// resize that shared context per call — see the Task 6 report's
    /// Concerns for the reasoning.
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
}

fn default_voice() -> String {
    "default".to_string()
}
fn default_timeout_secs() -> u64 {
    30
}
fn default_max_turns() -> usize {
    20
}

/// Resolve [`AgentProps::llm_widget`] — Task 4's ref-resolution template
/// applied to the agent composite: a non-empty ref is looked up in
/// `widgets`, erroring on a dangling ref or a `type_tag` other than `"llm"`;
/// on success only its `model_profile` is returned (its `system`/
/// `user_template` are irrelevant here — see the module docs on why the
/// agent's system prompt never comes from a widget). An empty ref returns
/// `Ok(None)`, telling the caller to fall back to the legacy
/// `agent_llm_profile`→`llm_profile` config chain — unlike
/// `dialog::resolve_llm_engine`, that fallback needs `AppConfig`, which this
/// pure function deliberately does not take, keeping it unit-testable
/// without a `State`.
pub(crate) fn resolve_agent_llm_widget_ref(
    props: &AgentProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.llm_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.llm_widget)
        .ok_or_else(|| format!("agent: llm_widget '{}' not found", props.llm_widget))?;
    if widget.type_tag != "llm" {
        return Err(format!(
            "agent llm_widget '{}' is a '{}' widget, expected 'llm'",
            props.llm_widget, widget.type_tag
        ));
    }
    let llm_props: fonos_core::workflow::llm_step::LlmProps =
        serde_json::from_value(widget.props.clone())
            .map_err(|e| format!("agent: llm_widget '{}' props: {e}", props.llm_widget))?;
    Ok(Some(llm_props.model_profile))
}

// ─── agent-panel JS bridge ──────────────────────────────────────────────────────

/// Run JS in the agent-panel webview (mirrors `dialog::dialog_js`).
///
/// Security note: `WebviewWindow::eval` injects JS into an app-owned Tauri
/// webview (the bundled `agent-panel.html`), not a general code-exec sink for
/// untrusted input. Every value interpolated into `js` is pre-escaped by
/// callers via `serde_json::to_string` (or manual quote/backslash escaping
/// for the single-quoted `recvError('...')` calls) before reaching this
/// function.
pub(crate) fn agent_js(h: &tauri::AppHandle, js: &str) {
    if let Some(panel) = h.get_webview_window("agent-panel") {
        if let Err(e) = panel.eval(js) {
            eprintln!("fonos: agent panel JS: {e}");
        }
    }
}

/// Play a synthesized WAV through the shared playback device, initializing it
/// on first use — mirrors `commands::tts::generate_and_play`'s "play
/// directly, no IPC round-trip" section. A poisoned lock or playback-init
/// failure is swallowed (the TTS synthesis already succeeded; a broken
/// speaker device shouldn't fail the whole exchange), matching the module's
/// "TTS is best-effort" framing. A standalone (non-async) function rather
/// than inlined at its one call site: it keeps the `state`/`guard` borrow
/// scoped to a plain function body instead of a `match` arm nested three
/// deep inside the exchange's async control flow.
fn play_wav_best_effort(app: &tauri::AppHandle, wav: Vec<u8>) {
    let state: tauri::State<'_, AppState> = app.state();
    let mut guard = match state.audio_playback.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    if guard.is_none() {
        if let Ok(p) = crate::audio::playback::AudioPlayback::new() {
            *guard = Some(p);
        }
    }
    if let Some(p) = guard.as_ref() {
        let _ = p.play_wav(wav);
    }
}

// ─── The shared exchange (non-empty-text path) ─────────────────────────────────

/// Run one full agent exchange for already-available `text` (an STT
/// transcript, or any other already-resolved input): grab the current
/// selection (BEFORE any panel show — see module docs), reveal the panel,
/// push the user message + a thinking indicator, run the skill-wielding
/// agent, push skill executions + the response, optionally speak it via TTS,
/// then auto-dismiss after playback settles.
///
/// `pub(crate)` so [`AgentOutput::deliver`]'s non-empty path is the only
/// current caller, but any future direct-text entry point could reuse it.
pub(crate) async fn run_agent_exchange(
    app: &tauri::AppHandle,
    text: String,
    props: &AgentProps,
) -> Result<(), String> {
    // Selection grab FIRST — before any panel show/focus — mirrors the
    // legacy arm's key-down-time grab (before the panel could steal focus).
    let sel = super::selection::grab_selection().await.ok();

    // Stop any TTS still playing from a previous exchange.
    {
        let state: tauri::State<'_, AppState> = app.state();
        let _ = super::tts::stop_playback(state);
    }

    super::move_agent_panel_to_cursor(app);
    if let Some(panel) = app.get_webview_window("agent-panel") {
        let _ = panel.show();
        let _ = panel.set_focus();
    }
    // Let the webview settle before eval() — same trick as dialog/text-action panels.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    // Reset persistent mode (hides header + mic footer if left over from a
    // prior blank-open `wf.agent` session) before the ephemeral exchange UI.
    agent_js(app, "recvShow(false)");

    // Selection-as-prompt-context prefix — logic copied from the legacy arm.
    let has_selection = sel.as_ref().map(|s| !s.text.is_empty()).unwrap_or(false);
    let agent_prompt = if let Some(ref s) = sel {
        if !s.text.is_empty() {
            format!(
                "[Selected text from {}]:\n\"\"\"\n{}\n\"\"\"\n\nUser instruction: {}",
                s.app_name, s.text, text
            )
        } else {
            text.clone()
        }
    } else {
        text.clone()
    };

    if has_selection {
        let sel_ref = sel.as_ref().unwrap();
        let preview: String = sel_ref.text.chars().take(120).collect();
        let sel_j = serde_json::to_string(&preview).unwrap_or_default();
        let app_j = serde_json::to_string(&sel_ref.app_name).unwrap_or_default();
        agent_js(app, &format!("recvSelection({sel_j}, {app_j})"));
    }
    let tx_j = serde_json::to_string(&text).unwrap_or_default();
    agent_js(app, &format!("recvUserMessage({tx_j})"));
    agent_js(app, "recvThinking()");

    // Resolve the LLM service: llm_widget ref wins (Task 4's template, type
    // check included); empty ref falls back to the legacy
    // agent_llm_profile→llm_profile chain, unchanged from `agent_process`.
    // Config lock scoped and dropped before any await below.
    let llm_service = {
        let state: tauri::State<'_, AppState> = app.state();
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let widgets = fonos_core::workflow::engine::effective_widgets(&config);
        let profile_id = match resolve_agent_llm_widget_ref(props, &widgets)? {
            Some(id) => id,
            None => {
                if !config.agent_llm_profile.is_empty() {
                    config.agent_llm_profile.clone()
                } else {
                    config.llm_profile.clone()
                }
            }
        };
        match super::agent::resolve_agent_llm_service(&config, &profile_id) {
            Ok(svc) => svc,
            Err(e) => {
                let esc = e.replace('\\', "\\\\").replace('\'', "\\'");
                agent_js(app, &format!("recvError('{esc}')"));
                return Err(e);
            }
        }
    };

    let agent_result = {
        let state: tauri::State<'_, AppState> = app.state();
        super::agent::run_agent_processor(&state, &agent_prompt, llm_service, Some(props.timeout_secs)).await
    };

    let result = match agent_result {
        Ok(r) => r,
        Err(e) => {
            let esc = e.replace('\\', "\\\\").replace('\'', "\\'");
            agent_js(app, &format!("recvError('{esc}')"));
            return Err(e);
        }
    };

    for exec in &result.skill_executions {
        let p_j = serde_json::to_string(&exec.params).unwrap_or_else(|_| "\"\"".into());
        let n_j = serde_json::to_string(&exec.skill_name).unwrap_or_default();
        agent_js(
            app,
            &format!("recvSkillExec({n_j},{p_j},{},{})", exec.latency_ms, exec.blocked),
        );
    }
    let r_j = serde_json::to_string(&result.response_text).unwrap_or_default();
    agent_js(app, &format!("recvResponse({r_j})"));

    // Optional TTS, per-widget props (voice/voice_profile — NOT cfg.default_voice;
    // speed is fixed at 1.0, matching workflow_widgets::SpeakOutput's own
    // simplification — the new widget system doesn't thread a per-call speed
    // override anywhere yet, agent included).
    let mut audio_dur = 0.0_f64;
    if props.tts_enabled && !result.response_text.is_empty() {
        // Truncate to the first 3 sentences for TTS — keep it brief.
        let tts_text = {
            let mut count = 0;
            let mut end = result.response_text.len();
            for (i, c) in result.response_text.char_indices() {
                if c == '.' || c == '!' || c == '?' || c == '\u{3002}' || c == '\u{ff01}' || c == '\u{ff1f}' {
                    count += 1;
                    if count >= 3 {
                        end = i + c.len_utf8();
                        break;
                    }
                }
            }
            result.response_text[..end].to_string()
        };

        let engine = {
            let state: tauri::State<'_, AppState> = app.state();
            let tts_svc = if props.voice_profile.is_empty() {
                super::get_service_config(&state, "tts")
            } else {
                super::get_service_config_for_profile(&state, &props.voice_profile)
            };
            (!tts_svc.base_url.trim().is_empty()).then(|| fonos_core::tts::HttpTts {
                service: tts_svc,
                voice: super::tts::resolve_voice(&props.voice),
                speed: 1.0,
            })
        };

        if let Some(engine) = engine {
            match fonos_core::listen::synthesize_long_text(&tts_text, &engine).await {
                Ok(wav) => {
                    audio_dur = fonos_core::listen::wav_duration_secs(&wav).unwrap_or(0.0);
                    play_wav_best_effort(app, wav);
                }
                Err(e) => eprintln!("fonos: agent TTS failed (non-fatal): {e}"),
            }
        }
    }

    // Auto-dismiss: wait for audio to finish + a 2s buffer.
    let app2 = app.clone();
    tokio::spawn(async move {
        let wait = audio_dur + 2.0;
        tokio::time::sleep(std::time::Duration::from_secs_f64(wait)).await;
        agent_js(&app2, "recvDismiss()");
    });

    Ok(())
}

// ─── AgentOutput ────────────────────────────────────────────────────────────────

/// `agent`: opens the floating agent-panel and, for non-empty input, runs
/// one skill-wielding exchange via [`run_agent_exchange`].
pub struct AgentOutput {
    /// Handle used to reach `AppState` and the panel window.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: AgentProps,
}

#[async_trait::async_trait]
impl Output for AgentOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        let text = match result {
            Data::Text(t) => t.clone(),
            Data::Audio(_) => return Err("agent output expected text, got audio".to_string()),
        };

        if text.is_empty() {
            // Blank-open (wf.agent, src.instant): position + reveal only —
            // the user drives the rest via the panel's own commands, exactly
            // like the legacy "agent-panel" toggle arm's show branch.
            super::move_agent_panel_to_cursor(&self.app);
            if let Some(panel) = self.app.get_webview_window("agent-panel") {
                let _ = panel.show();
                let _ = panel.set_focus();
            }
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            agent_js(&self.app, "recvShow(true)");
            return Ok(());
        }

        run_agent_exchange(&self.app, text, &self.props).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::workflow::model::WidgetRole;

    fn base_props() -> AgentProps {
        serde_json::from_value(serde_json::json!({})).unwrap()
    }

    fn llm_widget_def(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "model_profile": model_profile }),
            builtin: false,
        }
    }

    // ── AgentProps serde defaults ────────────────────────────────────────────

    #[test]
    fn agent_props_defaults_from_empty_json_mirror_config_defaults() {
        let props: AgentProps = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(props.llm_widget, "");
        assert!(!props.tts_enabled);
        assert_eq!(props.voice_profile, "");
        assert_eq!(props.voice, "default");
        assert_eq!(props.timeout_secs, 30);
        assert_eq!(props.max_turns, 20);
    }

    #[test]
    fn agent_props_old_json_without_new_fields_still_parses() {
        // Back-compat: a persisted agent widget missing any of these keys
        // (e.g. hand-authored, or from an earlier build) still deserializes.
        let json = r#"{"llm_widget": "llm.tuned"}"#;
        let props: AgentProps = serde_json::from_str(json).unwrap();
        assert_eq!(props.llm_widget, "llm.tuned");
        assert_eq!(props.voice, "default");
        assert_eq!(props.timeout_secs, 30);
        assert_eq!(props.max_turns, 20);
    }

    #[test]
    fn agent_props_roundtrip_preserves_custom_values() {
        let props = AgentProps {
            llm_widget: "llm.tuned".into(),
            tts_enabled: true,
            voice_profile: "tts.custom".into(),
            voice: "am_echo".into(),
            timeout_secs: 60,
            max_turns: 8,
        };
        let json = serde_json::to_value(&props).unwrap();
        let back: AgentProps = serde_json::from_value(json).unwrap();
        assert_eq!(back, props);
    }

    // ── resolve_agent_llm_widget_ref (Task 4 template, copied) ───────────────

    #[test]
    fn resolve_agent_llm_widget_ref_empty_returns_none() {
        let props = base_props();
        assert_eq!(resolve_agent_llm_widget_ref(&props, &[]).unwrap(), None);
    }

    #[test]
    fn resolve_agent_llm_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.llm_widget = "llm.tuned".into();
        let widgets = vec![llm_widget_def("llm.tuned", "tuned-profile")];
        assert_eq!(
            resolve_agent_llm_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-profile".to_string())
        );
    }

    #[test]
    fn resolve_agent_llm_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.llm_widget = "llm.missing".into();
        let err = resolve_agent_llm_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("llm.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_agent_llm_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.llm_widget = "speak.x".into();
        let wrong_type_widget = WidgetDef {
            id: "speak.x".to_string(),
            role: WidgetRole::Processor,
            type_tag: "speak".to_string(),
            name: "speak.x".to_string(),
            icon: String::new(),
            props: serde_json::json!({}),
            builtin: false,
        };
        let err = resolve_agent_llm_widget_ref(&props, &[wrong_type_widget]).unwrap_err();
        assert!(err.contains("expected 'llm'"), "error should mention type mismatch, got: {err}");
    }
}
