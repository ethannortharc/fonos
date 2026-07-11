//! Application configuration — load, save, and defaults.
//!
//! Config is stored as JSON at the platform's data directory:
//! `{data_dir}/com.fonos.app/config.json`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Error, Result};
use crate::modes::OutputTarget;
use crate::workflow::model::{WidgetDef, WorkflowDef};

/// Per-app override for the text injection strategy.
///
/// `app` is matched as a case-insensitive substring of the frontmost
/// application's name (e.g. `"terminal"` matches "Terminal" and "iTerm2" does
/// not). The first matching override in the list wins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct InjectionAppOverride {
    /// App name fragment to match (case-insensitive).
    pub app: String,
    /// `"paste"` (clipboard + Cmd+V) or `"type"` (simulated keystrokes).
    pub strategy: String,
}

/// A global-hotkey text action: grab the current selection, apply a mode's
/// LLM step, deliver the result to `output_target`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TextActionBinding {
    /// Hotkey combo, e.g. "cmd+shift+t". Empty disables the binding.
    pub hotkey: String,
    /// Mode id resolved via `modes::all_modes()` (e.g. "translate", "summarize").
    pub mode_id: String,
    /// Where the result is delivered. Bindings own the target so shared mode
    /// definitions (also used by dictation) keep their own behavior untouched.
    #[serde(default = "default_text_action_target")]
    pub output_target: OutputTarget,
}

fn default_text_action_target() -> OutputTarget { OutputTarget::FloatingPopup }

/// Application configuration persisted to disk as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Global hotkey combo for hold-to-talk dictation.
    pub hotkey_dictation: String,
    /// Global hotkey combo for toggle dictation (press to start, press to stop).
    pub hotkey_dictation_toggle: String,
    /// Global hotkey combo for triggering TTS playback.
    pub hotkey_tts: String,
    /// Default dictation processing mode (e.g. `"raw"`, `"polish"`).
    pub dictation_mode: String,
    /// Default TTS voice identifier.
    pub default_voice: String,
    /// TTS playback speed multiplier (1.0 = normal).
    pub tts_speed: f64,
    /// Preferred audio input device name, or `"default"`.
    pub audio_input_device: String,
    /// Preferred audio output device name, or `"default"`.
    pub audio_output_device: String,
    /// Whether to show the floating recording indicator pill.
    pub show_floating_indicator: bool,
    /// Ping the configured local STT/LLM backend when recording starts so the
    /// first capture after idle doesn't pay a model cold start (issue #4).
    pub warmup_enabled: bool,
    /// STT language hint (BCP-47 tag or `"auto"`).
    pub stt_language: String,
    /// Named model profiles: JSON array of `{id, name, provider, api_key, model, base_url, capabilities[]}`.
    pub model_profiles: Vec<serde_json::Value>,
    /// Which profile ID to use for speech-to-text.
    pub stt_profile: String,
    /// Which profile ID to use for text-to-speech.
    pub tts_profile: String,
    /// Which profile ID to use for LLM post-processing.
    pub llm_profile: String,
    /// System prompt used by the "clean" dictation mode.
    pub clean_prompt: String,
    /// Source language for translation mode (`"auto"` = detect).
    pub translate_source: String,
    /// Target language for translation mode.
    pub translate_target: String,

    // ── Agent settings ────────────────────────────────────────────────────

    /// Which model profile ID to use for agent LLM calls (independent from `llm_profile`).
    /// Empty string means "fall back to `llm_profile`".
    pub agent_llm_profile: String,
    /// DEPRECATED: system prompt formerly injected into every agent LLM
    /// request. Superseded by the per-widget `AgentProps::system` fallback
    /// (Workbench P2 Task 6 Fix Round 1 — mirrors `DialogProps`'s inline
    /// `system` field): `migrate_legacy_agent_triggers` copies a non-empty
    /// value here into the `agent.default` widget's `props.system` once,
    /// after which the agent exchange's resolution path never reads this
    /// field again. Kept only for that one-time migration read and for
    /// deserializing configs saved before this change; do not add new
    /// readers.
    pub agent_system_prompt: String,
    /// Extra commands to allow beyond the built-in safety allowlist.
    pub agent_safety_allowlist: Vec<String>,
    /// Extra commands to block beyond the built-in safety blocklist.
    pub agent_safety_blocklist: Vec<String>,
    /// Maximum wall-clock seconds allowed for a single skill execution (default 30).
    pub agent_timeout_secs: u64,
    /// Maximum number of user/assistant turn pairs to keep in conversation context (default 20).
    pub agent_max_turns: usize,
    /// Whether to speak agent responses via TTS after each reply.
    pub agent_tts_enabled: bool,
    /// Global hotkey combo for press-and-hold agent voice input.
    pub hotkey_agent: String,
    /// Global hotkey combo for toggling the agent panel view.
    pub hotkey_agent_panel: String,
    /// Global hotkey combo for toggling the note panel (note mode).
    pub hotkey_note: String,
    /// Notebook shortcut 1 hotkey combo (hold-to-talk into a specific notebook).
    pub hotkey_note_1: String,
    /// Notebook shortcut 2 hotkey combo.
    pub hotkey_note_2: String,
    /// Notebook shortcut 3 hotkey combo.
    pub hotkey_note_3: String,
    /// Container ID of notebook bound to shortcut 1 (0 = unbound).
    pub notebook_hotkey_1: i64,
    /// Container ID of notebook bound to shortcut 2 (0 = unbound).
    pub notebook_hotkey_2: i64,
    /// Container ID of notebook bound to shortcut 3 (0 = unbound).
    pub notebook_hotkey_3: i64,

    // ── Meeting settings ──────────────────────────────────────────────────

    /// Global hotkey combo for toggling meeting mode (Option+M). Folded into a
    /// `Trigger::Hotkey` chip on the `wf.meeting` composite recipe by
    /// [`crate::workflow::migrate::migrate_legacy_meeting_triggers`]
    /// (Workbench P2 Task 7), which clears this field afterward — mirrors
    /// `hotkey_agent`/`hotkey_agent_panel`'s own fold-then-clear treatment.
    pub hotkey_meeting: String,
    /// Which model profile ID to use for meeting AI summary generation.
    /// Empty string means "fall back to `llm_profile`".
    ///
    /// NOT fully superseded by `meeting.default`'s `llm_widget` ref (Workbench
    /// P2 Task 7): a model-profile id and a widget id live in different id
    /// spaces, so migration can't rewrite this into a ref the way it moved
    /// `agent_system_prompt` into `AgentProps::system` — see
    /// `commands::meeting_widget`'s resolve order (`llm_widget` ref →
    /// this field → `llm_profile`). Still read live at resolve time (a
    /// deliberate, documented deviation from the "no readers after migration"
    /// ideal — same id-space mismatch P1 hit with mode model overrides).
    pub meeting_llm_profile: String,
    /// Which model profile ID to use for meeting speech-to-text.
    /// Empty string means "fall back to the global `stt` profile".
    ///
    /// Same id-space caveat as `meeting_llm_profile` above: `meeting.default`'s
    /// `stt_widget` ref wins when set, this field is the next fallback, and
    /// the global `stt` profile is last (`commands::meeting_widget`'s STT
    /// resolution) — Workbench P2 Task 7's fix for the bug where
    /// `start_meeting` used to read the global `stt` profile unconditionally,
    /// silently ignoring this field entirely.
    pub meeting_stt_profile: String,
    /// DEPRECATED: custom system prompt for meeting summary generation,
    /// formerly read directly by `commands::meeting::build_summary_prompt`.
    /// Superseded by the per-widget `MeetingProps::summary_prompt` (Workbench
    /// P2 Task 7 — mirrors `agent_system_prompt`'s retirement in Task 6):
    /// `migrate_legacy_meeting_triggers` copies a non-empty value here into
    /// the `meeting.default` widget's `props.summary_prompt` once, after
    /// which the meeting composite's summary-prompt resolution never reads
    /// this field again (empty prop ⇒ the built-in literal, not this field).
    /// Kept only for that one-time migration read and for deserializing
    /// configs saved before this change; do not add new readers.
    pub meeting_summary_prompt: String,

    // ── Quick transform settings ─────────────────────────────────────────

    /// Global hotkey for quick-transform: select text → apply mode's LLM step → replace.
    pub hotkey_transform: String,
    /// Which dictation mode to use for quick-transform (e.g. "polish", "formal", "translate").
    /// Uses the mode's system prompt + user_template as the LLM processing step.
    pub transform_mode: String,
    /// Text-action bindings: hotkey → mode → output target. Replaces the
    /// legacy single `hotkey_transform`/`transform_mode` pair.
    #[serde(default)]
    pub text_actions: Vec<TextActionBinding>,

    // ── Text injection settings ──────────────────────────────────────────

    /// Default text injection strategy: `"paste"` (clipboard + Cmd+V, fast but
    /// briefly occupies the clipboard) or `"type"` (simulated keystrokes,
    /// never touches the clipboard but is slower for long text).
    pub injection_strategy: String,
    /// Per-app overrides of the injection strategy, matched against the
    /// frontmost app's name. First match wins.
    pub injection_app_overrides: Vec<InjectionAppOverride>,

    // ── Onboarding ───────────────────────────────────────────────────────

    /// Gates the first-run onboarding wizard. `false` until the user completes
    /// (or skips) the wizard; the wizard is shown on launch while this is false.
    pub has_completed_onboarding: bool,

    /// UI language: "auto" (follow system), "en", or "zh".
    pub ui_language: String,

    // ── Listen queue (issue #23) ─────────────────────────────────────────

    /// Global hotkey: capture the current selection into the Listen queue.
    pub hotkey_listen: String,
    /// DEPRECATED (Workbench P2 Task 10): mode id used to process captured
    /// text. No longer read — `commands::listen::do_create` always resolves
    /// the built-in `llm.listen` widget via
    /// [`crate::workflow::engine::effective_widgets`] instead (customization
    /// now happens by editing that widget, not by pointing Listen at a
    /// different mode id). A custom mode id parked here is NOT migrated onto
    /// the widget — an acknowledged edge case (see Task 10's report) — so a
    /// user who had customized Listen this way loses that customization. No
    /// remaining readers.
    pub listen_mode: String,
    /// TTS profile id for listen synthesis; empty = fall back to `tts_profile`.
    pub listen_voice_profile: String,
    /// Voice identifier for listen synthesis (provider-specific).
    pub listen_voice: String,

    // ── STS conversation (issue #24) — legacy fields, Workbench P2 Task 9 ──
    // The Talk page + STS walkie mode are retired; the hands-free call now
    // lives in the `call.default` composite widget (`CallProps`) and the
    // `wf.call` recipe. Most fields below are DEPRECATED: one-time-migrated by
    // [`crate::workflow::migrate::migrate_legacy_call_triggers`] and then
    // reset, with no live readers left. `sts_llm_profile` is the exception —
    // see its doc comment.

    /// DEPRECATED (Workbench P2 Task 9): global hotkey for the retired
    /// hold-to-talk conversation turn. One-time-migrated into a
    /// `Trigger::Hotkey` chip on `wf.call` (where the same key now toggles a
    /// hands-free call), then cleared. No remaining readers.
    pub hotkey_sts: String,
    /// DEPRECATED (Workbench P2 Task 9): persona / system prompt for the
    /// conversation chat stage. A non-empty, non-default value is one-time
    /// minted into a custom `llm` widget (「通话人格」, system = this field,
    /// model_profile = `sts_llm_profile`) referenced by `call.default`'s
    /// `llm_widget`; the default persona lives on as
    /// `commands::call_widget`'s built-in fallback constant. Left in place
    /// after migration (not cleared) so a hand-inspected config still shows
    /// where the minted persona came from — same treatment as
    /// `agent_system_prompt`.
    pub sts_persona: String,
    /// LLM profile id for the call's conversation stage; empty = fall back to
    /// `llm_profile`. DEPRECATED but still LIVE-READ (Workbench P2 Task 9):
    /// it names a model profile, not a widget, so — exactly like
    /// `meeting_stt_profile`/`meeting_llm_profile` — there is no ref it can
    /// faithfully become. It stays the middle rung of the call composite's
    /// resolve-time fallback chain (`llm_widget` ref → this field → global
    /// `"llm"`), a documented permanent exception to the "no readers after
    /// migration" ideal.
    pub sts_llm_profile: String,
    /// DEPRECATED (Workbench P2 Task 9): TTS profile id for the spoken reply.
    /// One-time-migrated into `call.default`'s `voice_profile` prop, then
    /// reset to its default. No remaining readers.
    pub sts_voice_profile: String,
    /// DEPRECATED (Workbench P2 Task 9): voice identifier for the spoken
    /// reply. One-time-migrated into `call.default`'s `voice` prop, then
    /// reset to its default. No remaining readers.
    pub sts_voice: String,
    /// DEPRECATED (Workbench P2 Task 9): conversation memory (max
    /// user/assistant turn pairs kept). One-time-migrated into
    /// `call.default`'s `max_turns` prop, then reset to its default. No
    /// remaining readers.
    pub sts_max_turns: usize,
    /// DEPRECATED (Workbench P2 Task 9): call-mode voice-activity sensitivity
    /// (0.0–1.0). One-time-migrated into `call.default`'s `vad_sensitivity`
    /// prop, then reset to its default (`0.5`). No remaining readers.
    pub call_vad_sensitivity: f32,
    /// DEPRECATED (Workbench P2 Task 9): trailing silence (ms) that ends an
    /// utterance (500–2000). One-time-migrated into `call.default`'s
    /// `vad_silence_ms` prop, then reset to its default (`800`). No remaining
    /// readers.
    pub call_vad_silence_ms: u32,
    /// DEPRECATED (Workbench P2 Task 9): allow the user to interrupt (barge
    /// in on) the spoken reply by speaking over it. One-time-migrated into
    /// `call.default`'s `barge_in` prop, then reset to its default (`true`).
    /// No remaining readers.
    pub call_barge_in: bool,

    // ── Custom vocabulary ────────────────────────────────────────────────

    /// User-defined vocab books (terms + correction rules). Referenced by id
    /// from `global_vocab_books` and per-mode `Mode.vocab_books`.
    pub vocab_books: Vec<crate::vocab::VocabBook>,
    /// Book ids applied to every dictation regardless of mode.
    pub global_vocab_books: Vec<String>,

    // ── Saved scenarios (issue #29) ──────────────────────────────────────

    /// Saved, switchable configuration bundles — each a self-contained
    /// snapshot of the profiles behind the default assignments plus those
    /// assignments. Applied / imported / exported from the Scenarios view.
    pub saved_scenarios: Vec<crate::scenarios::SavedScenario>,

    // ── Workflows (Workflow P1) ──────────────────────────────────────────

    /// Custom / overriding widget definitions. A config entry whose id matches
    /// a built-in replaces that built-in wholesale; entries with new ids are
    /// appended. See [`crate::workflow::engine::effective_widgets`].
    #[serde(default)]
    pub widgets: Vec<WidgetDef>,
    /// Custom / overriding workflow definitions, overlaid on the built-ins the
    /// same way as [`AppConfig::widgets`]. See
    /// [`crate::workflow::engine::effective_workflows`].
    #[serde(default)]
    pub workflows: Vec<WorkflowDef>,
    /// Set once the one-time migration of legacy hotkeys/modes into workflow
    /// defs has run, so it never runs again.
    #[serde(default)]
    pub workflow_migration_done: bool,
    /// Set once the one-time migration of formerly-global settings (STT
    /// language, insert strategy, translate target) into widget props has
    /// run, so it never runs again (Workflow P2). See
    /// [`crate::workflow::migrate::migrate_settings_into_flow`].
    #[serde(default)]
    pub settings_inflow_migration_done: bool,
    /// One-shot migration sentinel: legacy per-workflow `hotkey` strings
    /// converted into `triggers` (Workbench P1).
    #[serde(default)]
    pub triggers_migration_done: bool,
    /// Id of the voice (microphone-source) workflow that the pill hotkey
    /// (`config.pill_hotkey`) triggers. Empty falls back to the built-in
    /// `"wf.dictation"`. Written by the Dictation drum / float pill picker;
    /// read directly by the `"pill"` hotkey dispatch arm (`main.rs`) — every
    /// `workflow-{id}` hotkey label triggers its own id directly (see
    /// [`crate::workflow::engine::resolve_trigger_target`]).
    #[serde(default)]
    pub active_voice_workflow: String,

    // ── Pill hotkey (Workbench P1, spec §3c) ──────────────────────────────

    /// Global hotkey owned by the floating pill: pressing it runs the
    /// pill roller's currently selected workflow (`active_voice_workflow`,
    /// falling back to wf.dictation). Empty = unset.
    #[serde(default)]
    pub pill_hotkey: String,
    /// Key behavior for the pill hotkey: "hold" or "toggle".
    #[serde(default = "default_pill_hotkey_capture")]
    pub pill_hotkey_capture: String,
    /// One-shot migration sentinel: wf.dictation's first Hotkey chip moved
    /// to the pill (Workbench P1, spec §3c).
    #[serde(default)]
    pub pill_hotkey_migration_done: bool,
    /// One-shot migration sentinel: the legacy standalone Agent hotkeys
    /// (`hotkey_agent`/`hotkey_agent_panel`) folded into `Trigger::Hotkey`
    /// chips on the `wf.agent-voice`/`wf.agent` recipes (Workbench P2 Task 6).
    /// See [`crate::workflow::migrate::migrate_legacy_agent_triggers`].
    #[serde(default)]
    pub agent_triggers_migration_done: bool,
    /// One-shot migration sentinel: the legacy standalone `hotkey_meeting`
    /// folded into a `Trigger::Hotkey` chip on the `wf.meeting` recipe, and a
    /// non-empty legacy `meeting_summary_prompt` copied into the
    /// `meeting.default` widget's `props.summary_prompt` (Workbench P2 Task 7).
    /// See [`crate::workflow::migrate::migrate_legacy_meeting_triggers`].
    #[serde(default)]
    pub meeting_triggers_migration_done: bool,
    /// One-shot migration sentinel: the legacy `hotkey_sts` folded into a
    /// `Trigger::Hotkey` chip on the `wf.call` recipe, a non-default
    /// `sts_persona` minted into a custom `llm` widget referenced by
    /// `call.default`, and the legacy `sts_voice*`/`sts_max_turns`/`call_*`
    /// tuning seeded into `call.default`'s props (Workbench P2 Task 9). See
    /// [`crate::workflow::migrate::migrate_legacy_call_triggers`].
    #[serde(default)]
    pub call_triggers_migration_done: bool,
}

fn default_pill_hotkey_capture() -> String {
    "hold".into()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            hotkey_dictation: "cmd+shift+space".to_string(),
            hotkey_dictation_toggle: String::new(),
            hotkey_tts: "cmd+shift+s".to_string(),
            dictation_mode: "raw".to_string(),
            default_voice: "default".to_string(),
            tts_speed: 1.0,
            audio_input_device: "auto".to_string(),
            audio_output_device: "default".to_string(),
            show_floating_indicator: true,
            warmup_enabled: true,
            stt_language: "auto".to_string(),
            model_profiles: vec![],
            stt_profile: String::new(),
            tts_profile: String::new(),
            llm_profile: String::new(),
            clean_prompt: "Remove filler words (um, uh, 嗯, 就是说, 然后), fix punctuation and capitalization. Output ONLY the cleaned text, nothing else.".to_string(),
            translate_source: "auto".to_string(),
            translate_target: "English".to_string(),
            agent_llm_profile: String::new(),
            agent_system_prompt: "You are a helpful macOS desktop assistant. You can run shell commands, execute AppleScript, control apps, read the clipboard, and query system information. Your responses will be spoken aloud, so keep them to 1-2 sentences maximum. Give only the essential answer — no explanations, no caveats, no formatting. When you run a command, report just the key result.".to_string(),
            agent_safety_allowlist: Vec::new(),
            agent_safety_blocklist: Vec::new(),
            agent_timeout_secs: 30,
            agent_max_turns: 20,
            agent_tts_enabled: false,
            hotkey_agent: "cmd+shift+a".to_string(),
            hotkey_agent_panel: "cmd+shift+g".to_string(),
            hotkey_note: "option+n".to_string(),
            hotkey_note_1: "option+1".to_string(),
            hotkey_note_2: "option+2".to_string(),
            hotkey_note_3: "option+3".to_string(),
            notebook_hotkey_1: 0,
            notebook_hotkey_2: 0,
            notebook_hotkey_3: 0,
            hotkey_meeting: "option+m".to_string(),
            meeting_llm_profile: String::new(),
            meeting_stt_profile: String::new(),
            meeting_summary_prompt: String::new(),
            hotkey_transform: "cmd+shift+t".to_string(),
            ui_language: "auto".to_string(),
            hotkey_listen: "option+l".to_string(),
            listen_mode: "listen".to_string(),
            listen_voice_profile: String::new(),
            listen_voice: "default".to_string(),
            hotkey_sts: "option+s".to_string(),
            sts_persona: "You are a friendly voice assistant. Your replies are spoken aloud, so answer in 1-3 short conversational sentences in the user's language. Plain text only: no emoji, no markdown, no lists, no decorative symbols.".to_string(),
            sts_llm_profile: String::new(),
            sts_voice_profile: String::new(),
            sts_voice: "default".to_string(),
            sts_max_turns: 8,
            call_vad_sensitivity: 0.5,
            call_vad_silence_ms: 800,
            call_barge_in: true,
            transform_mode: "polish".to_string(),
            text_actions: Vec::new(),
            // Linux historically used xdotool type-first; macOS uses paste.
            injection_strategy: if cfg!(target_os = "linux") { "type" } else { "paste" }
                .to_string(),
            injection_app_overrides: Vec::new(),
            has_completed_onboarding: false,
            vocab_books: Vec::new(),
            global_vocab_books: Vec::new(),
            saved_scenarios: Vec::new(),
            widgets: Vec::new(),
            workflows: Vec::new(),
            workflow_migration_done: false,
            settings_inflow_migration_done: false,
            triggers_migration_done: false,
            active_voice_workflow: String::new(),
            pill_hotkey: String::new(),
            pill_hotkey_capture: default_pill_hotkey_capture(),
            pill_hotkey_migration_done: false,
            agent_triggers_migration_done: false,
            meeting_triggers_migration_done: false,
            call_triggers_migration_done: false,
        }
    }
}

impl AppConfig {
    /// Returns the directory where the config file is stored.
    ///
    /// On macOS this resolves to `~/Library/Application Support/com.fonos.app`.
    pub fn config_dir() -> PathBuf {
        dirs::data_dir()
            .expect("could not resolve data directory")
            .join("com.fonos.app")
    }

    /// Returns the full path to the config JSON file.
    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    /// Load config from disk.
    ///
    /// Missing fields fall back to defaults via `#[serde(default)]`.
    /// If the file does not exist or cannot be read, returns the default config.
    pub fn load() -> Self {
        let path = Self::config_path();
        match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the config to disk using an atomic write (write to temp file, then rename).
    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| Error::Config(e.to_string()))?;

        let path = Self::config_path();
        let tmp_path = path.with_extension("json.tmp");

        let json = serde_json::to_string_pretty(self).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::write(&tmp_path, &json).map_err(|e| Error::Config(e.to_string()))?;
        std::fs::rename(&tmp_path, &path).map_err(|e| Error::Config(e.to_string()))?;

        Ok(())
    }
}

/// One-time migration: convert the legacy quick-transform pair
/// (`hotkey_transform` + `transform_mode`) into a text-action binding.
///
/// Returns `true` if the config was modified — the caller should persist it.
pub fn migrate_transform_to_text_actions(config: &mut AppConfig) -> bool {
    if !config.text_actions.is_empty() || config.hotkey_transform.is_empty() {
        return false;
    }
    let mode_id = if config.transform_mode.is_empty() {
        "polish".to_string()
    } else {
        config.transform_mode.clone()
    };
    config.text_actions.push(TextActionBinding {
        hotkey: std::mem::take(&mut config.hotkey_transform),
        mode_id,
        output_target: OutputTarget::ActiveTextField,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_transform_creates_binding_and_clears_legacy_hotkey() {
        let mut cfg = AppConfig::default(); // default hotkey_transform = "cmd+shift+t"
        assert!(migrate_transform_to_text_actions(&mut cfg));
        assert_eq!(cfg.text_actions.len(), 1);
        let b = &cfg.text_actions[0];
        assert_eq!(b.hotkey, "cmd+shift+t");
        assert_eq!(b.mode_id, "polish");
        assert_eq!(b.output_target, crate::modes::OutputTarget::ActiveTextField);
        assert!(cfg.hotkey_transform.is_empty());
        // Idempotent: second call is a no-op.
        assert!(!migrate_transform_to_text_actions(&mut cfg));
    }

    #[test]
    fn migrate_transform_noop_when_disabled_or_already_migrated() {
        let mut disabled = AppConfig { hotkey_transform: String::new(), ..Default::default() };
        assert!(!migrate_transform_to_text_actions(&mut disabled));
        assert!(disabled.text_actions.is_empty());

        let mut existing = AppConfig::default();
        existing.text_actions.push(TextActionBinding {
            hotkey: "cmd+shift+y".into(),
            mode_id: "translate".into(),
            output_target: crate::modes::OutputTarget::FloatingPopup,
        });
        assert!(!migrate_transform_to_text_actions(&mut existing));
        assert_eq!(existing.text_actions.len(), 1);
    }
}
