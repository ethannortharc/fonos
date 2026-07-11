//! Session-type Call output: a hands-free voice call — listen → transcribe →
//! persona LLM → spoken reply, looping until hangup — folding the retired
//! Talk page's call button and the retired STS walkie mode's config into one
//! composite output (Workbench P2 Task 9).
//!
//! [`CallOutput`] is the terminal [`Output`] a `call` widget resolves to.
//! Like `meeting`, its `deliver` is a toggle: a live call
//! ([`super::call::is_call_active`]) hangs up; otherwise it resolves a
//! [`ResolvedCallCfg`], reveals the call-panel satellite window (Task 8's
//! `call-panel.html`, a full `sts:event` consumer), and starts the call loop.
//! A non-empty first-turn text (rare: a mic→STT-sourced recipe delivering
//! into `call`) is fed as the first transcript — one spoken exchange before
//! the listen loop takes over.
//!
//! **AUDIO RED LINE (this task's defining constraint):** the call loop's
//! audio machinery — `commands::call`'s VAD/barge/echo logic and
//! `audio::voice_capture`'s VPIO path — is behavior-frozen. This module only
//! changed the loop's *config-sourcing edges*: every `cfg.sts_*`/`call_*`
//! read that `execute_turn`/`run_call_loop` used to do is now front-loaded
//! into [`ResolvedCallCfg`], resolved exactly once, here, at deliver time
//! (the single constructor — the walkie path that used to build the same
//! values from config is deleted, so the compiler guarantees one source).
//!
//! Ref resolution (Task 4's template, applied per capability):
//! - [`CallProps::llm_widget`] non-empty → the referenced `llm` widget's
//!   `(model_profile, system)` pair, exactly like the agent composite
//!   (dangling ref / type mismatch are hard `Err`s on this start path). Its
//!   EMPTY `model_profile` falls through the cascade (T7 Fix Round 1's
//!   convention — never `resolve_profile("")`): the deprecated-but-live-read
//!   `sts_llm_profile` (id-space mismatch, same as the meeting profile
//!   fields) → the global `"llm"` profile. The ref's `system` is the persona
//!   verbatim (`None` ⇒ no system prompt — the ref replaces the whole
//!   persona, mirroring agent).
//! - [`CallProps::llm_widget`] empty → `sts_llm_profile` → global `"llm"`
//!   for the model, paired with [`DEFAULT_CALL_PERSONA`] (the retired
//!   `config.sts_persona` default, which `migrate_legacy_call_triggers`
//!   deliberately does not mint — a customized persona arrives here as a
//!   minted `llm.call-persona` ref instead).
//! - [`CallProps::stt_widget`] non-empty → the referenced `stt` widget's
//!   `model_profile` (empty falls through to the global `"stt"` profile —
//!   the retired sts-page transcribe path's behavior; the `"apple-speech"`
//!   sentinel IS usable here, unlike meeting's HTTP-only chunk loop, because
//!   the call transcribes through `dictation::transcribe_core`, which drives
//!   on-device recognition). Only the profile is threaded — the call's fixed
//!   transcribe pipeline does not consume the ref's prompt/vocab/temperature,
//!   same simplification as meeting.
//!
//! Lock discipline: the one `state.config.lock()` here is a tight scope
//! dropped before any await (same discipline as `agent_widget.rs`/
//! `meeting_widget.rs`).

use serde::{Deserialize, Serialize};
use tauri::Manager;

use fonos_core::config::AppConfig;
use fonos_core::llm::ServiceConfig;
use fonos_core::workflow::model::{Data, DataKind, WidgetDef};
use fonos_core::workflow::registry::{Output, RunCtx};

use super::AppState;

/// The retired `config.sts_persona` default — the call composite's built-in
/// persona when no `llm_widget` ref supplies one. Byte-locked against
/// `AppConfig::default().sts_persona` by a test below;
/// `migrate_legacy_call_triggers` mints a widget only for personas that
/// DIFFER from this, so the default case resolves here with no overlay.
pub(crate) const DEFAULT_CALL_PERSONA: &str = "You are a friendly voice assistant. Your replies are spoken aloud, so answer in 1-3 short conversational sentences in the user's language. Plain text only: no emoji, no markdown, no lists, no decorative symbols.";

// ─── CallProps ────────────────────────────────────────────────────────────────

/// Configuration for a `call`-type composite widget's output. Serde defaults
/// mirror the legacy config defaults byte-for-byte (`sts_voice*`,
/// `sts_max_turns`, `call_vad_*`, `call_barge_in`) so a bare `{}` — or the
/// shipped `call.default` builtin — behaves exactly like the retired config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CallProps {
    /// Id of a tuned `stt` widget whose `model_profile` transcribes the
    /// call's utterances; empty = the global `"stt"` profile (the retired
    /// sts-page path's behavior). See [`resolve_call_stt_widget_ref`].
    #[serde(default)]
    pub stt_widget: String,
    /// Id of a tuned `llm` widget supplying the call's model profile AND
    /// persona (`system`); empty = the `sts_llm_profile`→global-`"llm"`
    /// chain + [`DEFAULT_CALL_PERSONA`]. See [`resolve_call_llm_widget_ref`].
    #[serde(default)]
    pub llm_widget: String,
    /// TTS profile id for the spoken reply, or empty for the global `"tts"`
    /// profile (same convention as `SpeakOutput::voice_profile`).
    #[serde(default)]
    pub voice_profile: String,
    /// Voice identifier for the spoken reply (`"default"` resolves via
    /// `tts::resolve_voice`, same as `SpeakOutput::voice`).
    #[serde(default = "default_voice")]
    pub voice: String,
    /// Conversation memory: max user/assistant turn pairs kept.
    #[serde(default = "default_max_turns")]
    pub max_turns: usize,
    /// Voice-activity sensitivity (0.0–1.0); higher ends turns sooner.
    #[serde(default = "default_vad_sensitivity")]
    pub vad_sensitivity: f32,
    /// Trailing silence (ms) that ends an utterance (clamped 500–2000 by the
    /// loop, unchanged).
    #[serde(default = "default_vad_silence_ms")]
    pub vad_silence_ms: u32,
    /// Allow interrupting the spoken reply by talking over it (barge-in).
    #[serde(default = "default_barge_in")]
    pub barge_in: bool,
}

fn default_voice() -> String {
    "default".to_string()
}
fn default_max_turns() -> usize {
    8
}
fn default_vad_sensitivity() -> f32 {
    0.5
}
fn default_vad_silence_ms() -> u32 {
    800
}
fn default_barge_in() -> bool {
    true
}

// ─── ResolvedCallCfg ──────────────────────────────────────────────────────────

/// Everything the call loop + turn executor used to read from `cfg.sts_*` /
/// `cfg.call_*`, resolved once at deliver time. [`resolve_call_cfg`] is the
/// ONLY constructor (the walkie path that used to build these values inline
/// per turn is deleted), so `sts::execute_turn_with_audio` and
/// `call::run_call_loop` have a single, compiler-checked config source.
///
/// Deliberately NOT here (still live-read per turn, preserving behavior):
/// the vocab books (`execute_turn_with_audio` re-reads them each turn, so
/// mid-call vocab edits keep applying) and `audio_input_device`
/// (`run_call_loop` snapshots it at loop start, as before).
#[derive(Debug, Clone)]
pub struct ResolvedCallCfg {
    /// System prompt for the conversation chat stage. Empty = no persona
    /// (an explicit ref with no `system`), matching `ChatStage`'s existing
    /// empty-string semantics.
    pub persona_system: String,
    /// Resolved conversation LLM connection.
    pub llm: ServiceConfig,
    /// Resolved TTS connection for the spoken reply.
    pub tts: ServiceConfig,
    /// Voice identifier (resolved to a reference-audio path at turn time via
    /// `tts::resolve_voice`, unchanged).
    pub voice: String,
    /// Conversation memory: max user/assistant turn pairs kept.
    pub max_turns: usize,
    /// STT profile id for the listen loop's transcriptions (may be the
    /// `"apple-speech"` sentinel); `None` = the global `"stt"` profile —
    /// exactly the retired `Some("sts-page")` mode's resolution.
    pub stt_profile: Option<String>,
    /// Voice-activity sensitivity (0.0–1.0).
    pub vad_sensitivity: f32,
    /// Trailing silence (ms) that ends an utterance.
    pub vad_silence_ms: u32,
    /// Barge-in enabled.
    pub barge_in: bool,
}

/// Resolve [`CallProps::llm_widget`] — Task 4's ref-resolution template,
/// agent's flavor (both `model_profile` AND `system` come back): a non-empty
/// ref is looked up in `widgets`, erroring on a dangling ref or a `type_tag`
/// other than `"llm"`. An empty ref returns `Ok(None)` — the caller falls
/// back to the `sts_llm_profile`→global chain + [`DEFAULT_CALL_PERSONA`].
pub(crate) fn resolve_call_llm_widget_ref(
    props: &CallProps,
    widgets: &[WidgetDef],
) -> Result<Option<(String, Option<String>)>, String> {
    if props.llm_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.llm_widget)
        .ok_or_else(|| format!("call: llm_widget '{}' not found", props.llm_widget))?;
    if widget.type_tag != "llm" {
        return Err(format!(
            "call llm_widget '{}' is a '{}' widget, expected 'llm'",
            props.llm_widget, widget.type_tag
        ));
    }
    let llm_props: fonos_core::workflow::llm_step::LlmProps =
        serde_json::from_value(widget.props.clone())
            .map_err(|e| format!("call: llm_widget '{}' props: {e}", props.llm_widget))?;
    Ok(Some((llm_props.model_profile, llm_props.system)))
}

/// Resolve [`CallProps::stt_widget`] — same template, `stt` capability
/// (mirrors `meeting_widget::resolve_meeting_stt_widget_ref`): a non-empty
/// ref yields its `model_profile` verbatim (possibly empty or the
/// `"apple-speech"` sentinel — see [`resolve_call_stt_tier`]); an empty ref
/// returns `Ok(None)`.
pub(crate) fn resolve_call_stt_widget_ref(
    props: &CallProps,
    widgets: &[WidgetDef],
) -> Result<Option<String>, String> {
    if props.stt_widget.is_empty() {
        return Ok(None);
    }
    let widget = widgets
        .iter()
        .find(|w| w.id == props.stt_widget)
        .ok_or_else(|| format!("call: stt_widget '{}' not found", props.stt_widget))?;
    if widget.type_tag != "stt" {
        return Err(format!(
            "call stt_widget '{}' is a '{}' widget, expected 'stt'",
            props.stt_widget, widget.type_tag
        ));
    }
    let stt_props: super::workflow_widgets::SttProps = serde_json::from_value(widget.props.clone())
        .map_err(|e| format!("call: stt_widget '{}' props: {e}", props.stt_widget))?;
    Ok(Some(stt_props.model_profile))
}

/// The effective STT profile tier: a resolved ref whose own `model_profile`
/// is non-empty wins (the `"apple-speech"` sentinel included — the call's
/// transcribe path CAN drive on-device recognition, unlike meeting's
/// HTTP-only loop); an empty ref profile — `stt.default`'s "use global"
/// convention — falls through exactly like an absent ref (T7 Fix Round 1's
/// convention). `None` means the global `"stt"` profile, the retired
/// sts-page path's behavior (there is no legacy call-STT config field to
/// fall back to in between).
pub(crate) fn resolve_call_stt_tier(ref_profile: Option<&str>) -> Option<String> {
    ref_profile.filter(|p| !p.is_empty()).map(|p| p.to_string())
}

/// The effective LLM profile tier + persona, from
/// [`resolve_call_llm_widget_ref`]'s `Ok` payload (pure — unit-testable
/// without an `AppConfig`). Returns `(profile_tier, persona_system)`:
/// `profile_tier` is `None` for "resolve the global `\"llm\"` service".
///
/// - Ref present: its non-empty `model_profile` wins; an EMPTY one falls
///   through the same cascade as no ref (never resolve an empty profile id).
///   The ref's `system` is the persona verbatim — `None` ⇒ empty ⇒ no
///   persona (the ref replaces the whole persona, mirroring agent's ref
///   semantics).
/// - No ref: the deprecated-but-live-read `sts_llm_profile` → global, with
///   [`DEFAULT_CALL_PERSONA`] as the persona.
pub(crate) fn resolve_call_llm_tier(
    ref_payload: Option<(String, Option<String>)>,
    sts_llm_profile: &str,
) -> (Option<String>, String) {
    let legacy_tier = || {
        if sts_llm_profile.is_empty() {
            None
        } else {
            Some(sts_llm_profile.to_string())
        }
    };
    match ref_payload {
        Some((model_profile, system)) => {
            let tier = if model_profile.is_empty() { legacy_tier() } else { Some(model_profile) };
            (tier, system.unwrap_or_default())
        }
        None => (legacy_tier(), DEFAULT_CALL_PERSONA.to_string()),
    }
}

/// Build the [`ResolvedCallCfg`] for `props` against a config snapshot — the
/// single constructor, called by [`CallOutput::deliver`] inside one scoped
/// config lock. Pure over `&AppConfig`, so the whole resolution is
/// unit-testable without a `tauri::State`.
pub(crate) fn resolve_call_cfg(config: &AppConfig, props: &CallProps) -> Result<ResolvedCallCfg, String> {
    let widgets = fonos_core::workflow::engine::effective_widgets(config);

    let llm_ref = resolve_call_llm_widget_ref(props, &widgets)?;
    let (llm_tier, persona_system) = resolve_call_llm_tier(llm_ref, &config.sts_llm_profile);
    let llm = match llm_tier {
        Some(profile_id) => fonos_core::services::resolve_profile(config, &profile_id),
        None => fonos_core::services::resolve_service(config, "llm"),
    };

    let stt_ref = resolve_call_stt_widget_ref(props, &widgets)?;
    let stt_profile = resolve_call_stt_tier(stt_ref.as_deref());

    // TTS: same voice_profile→global-"tts" convention as SpeakOutput / the
    // retired execute_turn's sts_voice_profile read.
    let tts = if props.voice_profile.is_empty() {
        fonos_core::services::resolve_service(config, "tts")
    } else {
        fonos_core::services::resolve_profile(config, &props.voice_profile)
    };

    Ok(ResolvedCallCfg {
        persona_system,
        llm,
        tts,
        voice: props.voice.clone(),
        max_turns: props.max_turns,
        stt_profile,
        vad_sensitivity: props.vad_sensitivity,
        vad_silence_ms: props.vad_silence_ms,
        barge_in: props.barge_in,
    })
}

// ─── CallOutput ───────────────────────────────────────────────────────────────

/// `call`: toggle a hands-free voice call — hang up when one is live, else
/// resolve the config, reveal the call panel, and start the loop.
pub struct CallOutput {
    /// Handle used to reach `AppState` and the call-panel window.
    pub app: tauri::AppHandle,
    /// Deserialized widget configuration.
    pub props: CallProps,
}

#[async_trait::async_trait]
impl Output for CallOutput {
    fn accepts(&self) -> DataKind {
        DataKind::Text
    }

    async fn deliver(&self, result: &Data, _ctx: &RunCtx) -> Result<(), String> {
        let text = match result {
            Data::Text(t) => t.clone(),
            Data::Audio(_) => return Err("call output expected text, got audio".to_string()),
        };

        let state: tauri::State<'_, AppState> = self.app.state();

        // Live call ⇒ this delivery is the hang-up half of the toggle. No
        // resolution happens on this path, so a broken ref can never block
        // hanging up (T7 Fix Round 1's stop-path rule, upheld structurally).
        if super::call::is_call_active(&state) {
            return super::call::call_stop(state).await;
        }

        // Resolve everything up front in one scoped config lock (hard Err on
        // a dangling/mismatched ref — this is a start path).
        let call_cfg = {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            resolve_call_cfg(&config, &self.props)?
        };

        // Reveal the call-panel satellite (Task 8's window; 380×520 in
        // tauri.conf.json). The panel is a pure sts:event consumer — no eval
        // handshake, so no settle sleep is needed before starting the loop.
        #[cfg(target_os = "macos")]
        super::move_call_panel_to_cursor(&self.app, 380, 520);
        if let Some(panel) = self.app.get_webview_window("call-panel") {
            let _ = panel.show();
            let _ = panel.set_focus();
        }

        // Start the loop, seeding a non-empty first-turn text as the first
        // transcript (spoken exchange before the listen loop takes over).
        let first = Some(text).filter(|t| !t.trim().is_empty());
        super::call::start_call(self.app.clone(), &state, call_cfg, first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::workflow::model::WidgetRole;

    fn base_props() -> CallProps {
        serde_json::from_value(serde_json::json!({})).unwrap()
    }

    fn llm_widget_def(id: &str, model_profile: &str, system: Option<&str>) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "model_profile": model_profile, "system": system }),
            builtin: false,
        }
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

    // ── CallProps serde defaults ─────────────────────────────────────────────

    #[test]
    fn call_props_defaults_from_empty_json_mirror_config_defaults() {
        let props = base_props();
        let cfg = AppConfig::default();
        assert_eq!(props.stt_widget, "");
        assert_eq!(props.llm_widget, "");
        assert_eq!(props.voice_profile, cfg.sts_voice_profile);
        assert_eq!(props.voice, cfg.sts_voice);
        assert_eq!(props.max_turns, cfg.sts_max_turns);
        assert_eq!(props.vad_sensitivity, cfg.call_vad_sensitivity);
        assert_eq!(props.vad_silence_ms, cfg.call_vad_silence_ms);
        assert_eq!(props.barge_in, cfg.call_barge_in);
    }

    #[test]
    fn call_props_old_json_without_new_fields_still_parses() {
        let json = r#"{"llm_widget": "llm.tuned"}"#;
        let props: CallProps = serde_json::from_str(json).unwrap();
        assert_eq!(props.llm_widget, "llm.tuned");
        assert_eq!(props.voice, "default");
        assert_eq!(props.max_turns, 8);
        assert!(props.barge_in);
    }

    #[test]
    fn call_props_roundtrip_preserves_custom_values() {
        let props = CallProps {
            stt_widget: "stt.tuned".into(),
            llm_widget: "llm.tuned".into(),
            voice_profile: "tts.custom".into(),
            voice: "af_bella".into(),
            max_turns: 4,
            vad_sensitivity: 0.8,
            vad_silence_ms: 1200,
            barge_in: false,
        };
        let json = serde_json::to_value(&props).unwrap();
        let back: CallProps = serde_json::from_value(json).unwrap();
        assert_eq!(back, props);
    }

    /// The built-in call.default widget's props must deserialize into
    /// CallProps with exactly the serde defaults (two independent copies of
    /// the same numbers — lock them together).
    #[test]
    fn builtin_call_default_props_deserialize_to_default_call_props() {
        let w = fonos_core::workflow::builtin::built_in_widgets()
            .into_iter()
            .find(|w| w.id == "call.default")
            .expect("call.default builtin");
        let props: CallProps = serde_json::from_value(w.props).expect("call.default props are CallProps");
        assert_eq!(props, base_props());
    }

    /// DEFAULT_CALL_PERSONA is byte-locked to the legacy config default it
    /// replaces: migrate_legacy_call_triggers mints a widget only for
    /// personas that DIFFER from AppConfig::default().sts_persona, so this
    /// constant must stay identical or default-persona users would drift.
    #[test]
    fn default_call_persona_matches_legacy_config_default() {
        assert_eq!(DEFAULT_CALL_PERSONA, AppConfig::default().sts_persona);
    }

    // ── resolve_call_llm_widget_ref ──────────────────────────────────────────

    #[test]
    fn resolve_call_llm_widget_ref_empty_returns_none() {
        assert_eq!(resolve_call_llm_widget_ref(&base_props(), &[]).unwrap(), None);
    }

    #[test]
    fn resolve_call_llm_widget_ref_resolves_profile_and_system() {
        let mut props = base_props();
        props.llm_widget = "llm.tuned".into();
        let widgets = vec![llm_widget_def("llm.tuned", "tuned-profile", Some("You are Rex."))];
        assert_eq!(
            resolve_call_llm_widget_ref(&props, &widgets).unwrap(),
            Some(("tuned-profile".to_string(), Some("You are Rex.".to_string())))
        );
    }

    #[test]
    fn resolve_call_llm_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.llm_widget = "llm.missing".into();
        let err = resolve_call_llm_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("llm.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_call_llm_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.llm_widget = "stt.x".into();
        let err = resolve_call_llm_widget_ref(&props, &[stt_widget_def("stt.x", "p")]).unwrap_err();
        assert!(err.contains("expected 'llm'"), "error should mention type mismatch, got: {err}");
    }

    // ── resolve_call_stt_widget_ref / resolve_call_stt_tier ─────────────────

    #[test]
    fn resolve_call_stt_widget_ref_empty_returns_none() {
        assert_eq!(resolve_call_stt_widget_ref(&base_props(), &[]).unwrap(), None);
    }

    #[test]
    fn resolve_call_stt_widget_ref_resolves_model_profile() {
        let mut props = base_props();
        props.stt_widget = "stt.tuned".into();
        let widgets = vec![stt_widget_def("stt.tuned", "tuned-stt-profile")];
        assert_eq!(
            resolve_call_stt_widget_ref(&props, &widgets).unwrap(),
            Some("tuned-stt-profile".to_string())
        );
    }

    #[test]
    fn resolve_call_stt_widget_ref_dangling_ref_errors() {
        let mut props = base_props();
        props.stt_widget = "stt.missing".into();
        let err = resolve_call_stt_widget_ref(&props, &[]).unwrap_err();
        assert!(err.contains("stt.missing"), "error should name the missing id, got: {err}");
    }

    #[test]
    fn resolve_call_stt_widget_ref_type_mismatch_errors() {
        let mut props = base_props();
        props.stt_widget = "llm.x".into();
        let err = resolve_call_stt_widget_ref(&props, &[llm_widget_def("llm.x", "p", None)]).unwrap_err();
        assert!(err.contains("expected 'stt'"), "error should mention type mismatch, got: {err}");
    }

    #[test]
    fn resolve_call_stt_tier_empty_ref_profile_falls_to_global() {
        // stt.default's "use global" convention: an empty ref profile must
        // NOT be resolved blindly (T7 Fix Round 1's rule).
        assert_eq!(resolve_call_stt_tier(Some("")), None);
        assert_eq!(resolve_call_stt_tier(None), None);
    }

    #[test]
    fn resolve_call_stt_tier_usable_ref_wins_including_apple_speech() {
        assert_eq!(resolve_call_stt_tier(Some("tuned")), Some("tuned".to_string()));
        // Unlike meeting's HTTP-only loop, the call's transcribe path drives
        // on-device recognition, so the sentinel is a usable tier here.
        assert_eq!(resolve_call_stt_tier(Some("apple-speech")), Some("apple-speech".to_string()));
    }

    // ── resolve_call_llm_tier ────────────────────────────────────────────────

    #[test]
    fn resolve_call_llm_tier_ref_profile_and_system_win() {
        let (tier, persona) = resolve_call_llm_tier(
            Some(("tuned-profile".into(), Some("You are Rex.".into()))),
            "legacy-profile",
        );
        assert_eq!(tier, Some("tuned-profile".to_string()));
        assert_eq!(persona, "You are Rex.");
    }

    #[test]
    fn resolve_call_llm_tier_empty_ref_profile_falls_to_legacy_then_global() {
        // Ref present but its model_profile is empty ("use global"
        // convention) — the model falls through the cascade while the ref's
        // system still owns the persona.
        let (tier, persona) =
            resolve_call_llm_tier(Some((String::new(), Some("You are Rex.".into()))), "legacy-profile");
        assert_eq!(tier, Some("legacy-profile".to_string()));
        assert_eq!(persona, "You are Rex.");

        let (tier, _) = resolve_call_llm_tier(Some((String::new(), None)), "");
        assert_eq!(tier, None, "empty ref profile + empty legacy field ⇒ global");
    }

    #[test]
    fn resolve_call_llm_tier_ref_without_system_means_no_persona() {
        // The ref replaces the whole persona (agent semantics): a ref with no
        // system yields an empty persona, not the built-in default.
        let (_, persona) = resolve_call_llm_tier(Some(("p".into(), None)), "");
        assert_eq!(persona, "");
    }

    #[test]
    fn resolve_call_llm_tier_no_ref_uses_legacy_field_and_default_persona() {
        let (tier, persona) = resolve_call_llm_tier(None, "legacy-profile");
        assert_eq!(tier, Some("legacy-profile".to_string()));
        assert_eq!(persona, DEFAULT_CALL_PERSONA);

        let (tier, persona) = resolve_call_llm_tier(None, "");
        assert_eq!(tier, None);
        assert_eq!(persona, DEFAULT_CALL_PERSONA);
    }

    // ── resolve_call_cfg (full resolution over an AppConfig snapshot) ───────

    /// A config with a model profile so the tier resolution is observable.
    fn config_with_profiles() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(serde_json::json!({
            "id": "tuned-llm",
            "name": "Tuned",
            "provider": "openai",
            "base_url": "http://tuned.example",
            "api_key": "",
            "model": "tuned-model",
            "capabilities": ["llm"],
        }));
        cfg
    }

    #[test]
    fn resolve_call_cfg_defaults_resolve_global_services_and_default_persona() {
        let cfg = config_with_profiles();
        let resolved = resolve_call_cfg(&cfg, &base_props()).unwrap();
        assert_eq!(resolved.persona_system, DEFAULT_CALL_PERSONA);
        assert_eq!(resolved.stt_profile, None);
        assert_eq!(resolved.voice, "default");
        assert_eq!(resolved.max_turns, 8);
        assert_eq!(resolved.vad_sensitivity, 0.5);
        assert_eq!(resolved.vad_silence_ms, 800);
        assert!(resolved.barge_in);
    }

    #[test]
    fn resolve_call_cfg_llm_ref_supplies_model_and_persona() {
        let mut cfg = config_with_profiles();
        cfg.widgets.push(llm_widget_def("llm.call-persona", "tuned-llm", Some("You are Rex.")));
        let mut props = base_props();
        props.llm_widget = "llm.call-persona".into();
        let resolved = resolve_call_cfg(&cfg, &props).unwrap();
        assert_eq!(resolved.persona_system, "You are Rex.");
        assert_eq!(resolved.llm.base_url, "http://tuned.example");
        assert_eq!(resolved.llm.model, "tuned-model");
    }

    #[test]
    fn resolve_call_cfg_dangling_llm_ref_is_hard_err() {
        let cfg = config_with_profiles();
        let mut props = base_props();
        props.llm_widget = "llm.missing".into();
        assert!(resolve_call_cfg(&cfg, &props).is_err());
    }

    #[test]
    fn resolve_call_cfg_stt_ref_threads_profile_and_empty_falls_through() {
        let mut cfg = config_with_profiles();
        cfg.widgets.push(stt_widget_def("stt.tuned", "tuned-stt"));
        cfg.widgets.push(stt_widget_def("stt.blank", ""));

        let mut props = base_props();
        props.stt_widget = "stt.tuned".into();
        assert_eq!(resolve_call_cfg(&cfg, &props).unwrap().stt_profile, Some("tuned-stt".to_string()));

        props.stt_widget = "stt.blank".into();
        assert_eq!(resolve_call_cfg(&cfg, &props).unwrap().stt_profile, None);
    }
}
