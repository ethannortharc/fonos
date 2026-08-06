//! Application configuration — load, save, and defaults.
//!
//! Config is stored as JSON at the platform's data directory:
//! `{data_dir}/com.fonos.app/config.json`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{Error, Result};
use crate::workflow::model::{WidgetDef, WorkflowDef};

/// Where a text-action's processed result is sent.
///
/// Formerly declared alongside the legacy `modes` system (deleted in
/// Workbench P2 Task 12); moved here because [`TextActionBinding`] is its
/// only remaining consumer (`modes::ContainerKind`, the other half of that
/// system's v2 output shape, had none left and was dropped entirely).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputTarget {
    /// Copy result to the system clipboard.
    #[default]
    Clipboard,
    /// Type/paste result into the currently-focused text field.
    ActiveTextField,
    /// Append result as a new Entry in a Container (notebook/conversation).
    AppendToContainer,
    /// Show result in a floating popup panel near the mouse cursor.
    /// Core only declares the intent — rendering is a desktop adapter concern.
    FloatingPopup,
    /// Discard output — entry is saved to DB but not sent anywhere.
    None,
}

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
    /// Legacy mode id (e.g. "translate", "summarize"). Read only by
    /// [`crate::workflow::migrate::migrate_to_workflows`]'s one-time
    /// conversion into a `wf.ta-*` workflow referencing the matching
    /// `llm.{mode_id}` processor widget — the modes system this once
    /// resolved through (`modes::all_modes()`) was deleted in Workbench P2
    /// Task 12.
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
    /// DEPRECATED — superseded by [`AppConfig::float_indicator`], which folds
    /// "don't show it" into the same field as the shape (`"off"`). Never had a
    /// reader: no code path ever hid the float window on this being `false`.
    /// Kept only so configs written before `float_indicator` existed still
    /// deserialize, and for the one-time migration in [`migrated_float_indicator`]
    /// that turns a `false` here into `"off"`. Do not add new readers.
    pub show_floating_indicator: bool,
    /// Idle shape of the floating indicator — the single knob for how much
    /// screen the pill occupies when it has nothing to say. One of
    /// [`FLOAT_HAIRLINE`] / [`FLOAT_DOT`] / [`FLOAT_PILL`] / [`FLOAT_OFF`];
    /// unknown values normalize to the default (see [`AppConfig::float_form`]).
    ///
    /// A `String` rather than an enum, and read through a lenient
    /// deserializer, for the same reason: [`AppConfig::load`] ends in
    /// `unwrap_or_default()`, so ANY value that fails to deserialize takes the
    /// whole struct down and silently resets every unrelated setting. An enum
    /// would reject an unknown shape; a plain `String` would still reject
    /// `null` or a number. [`lenient`] contains those, so a bad value costs
    /// this field and nothing else. (It cannot contain *everything* — a numeric
    /// literal outside f64's range fails inside serde_json's own number
    /// parsing, before any field handler runs; [`AppConfig::load`] backs the
    /// file up before falling back for that case.)
    ///
    /// Only the IDLE shape is affected — the working states (recording,
    /// processing, result) always expand to the full pill, since that is
    /// exactly when the occluded pixels are the ones you want. That holds for
    /// `"off"` too: it means "no idle presence", not "no feedback", so the pill
    /// still surfaces for a dictation and disappears when it finishes.
    /// Withholding it entirely would mean pressing a hotkey and watching text
    /// appear with nothing on screen acknowledging it.
    #[serde(default, deserialize_with = "lenient")]
    pub float_indicator: String,
    /// Seconds of idleness before the indicator fades to its dormant level.
    /// `0` disables dormancy (it stays at the resting opacity indefinitely).
    #[serde(default = "default_idle_fade_secs", deserialize_with = "lenient_idle_fade_secs")]
    pub float_idle_fade_secs: u32,
    /// Remembered indicator position as the *center* of the visible mark, in
    /// **logical points** — `None` until the user drags it, after which it is
    /// the anchor every resize positions from.
    ///
    /// Center rather than top-left because the window changes size between
    /// states (a 44×14 hairline grows into a 122×32 recording pill): anchoring
    /// the center makes the pill bloom symmetrically out of the mark, and keeps
    /// a dragged position from drifting as the window resizes around it.
    ///
    /// Points rather than physical pixels because a multi-display desktop has
    /// no coherent global *physical* space — see the coordinate-space notes in
    /// the desktop crate's `commands::float_geom`.
    #[serde(default, deserialize_with = "lenient")]
    pub float_anchor_x: Option<f64>,
    /// See [`AppConfig::float_anchor_x`].
    #[serde(default, deserialize_with = "lenient")]
    pub float_anchor_y: Option<f64>,
    /// Ping the configured local STT/LLM backend when recording starts so the
    /// first capture after idle doesn't pay a model cold start (issue #4).
    pub warmup_enabled: bool,
    /// DEPRECATED (Workbench P2 Task 12, audited alongside the legacy
    /// `modes` system's deletion): STT language hint (BCP-47 tag or
    /// `"auto"`). No remaining readers — `crate::workflow::migrate::migrate_settings_into_flow`
    /// (Workflow P2) one-time-baked this into every `stt`-type widget's own
    /// `language` prop (`stt.default`'s `SttProps::language`, read live by
    /// `workflow_widgets::SttProcessor` and by `commands::dictation`'s
    /// dictation-flow STT step, both of which read the widget prop, never
    /// this field). Kept only for that one-time migration read and for
    /// deserializing configs saved before it ran; do not add new readers.
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
    /// DEPRECATED (Workbench P2 Task 12, audited alongside the legacy
    /// `modes` system's deletion): target language for translation mode. No
    /// remaining runtime readers — the `llm.translate` widget's own prompt
    /// template carries the target now (`migrate_settings_into_flow` baked a
    /// non-default value in once; `workflow_widgets::LlmProcessor` and
    /// `commands::llm::run_dictation_llm_step` both pass `""` for
    /// `translate_target` to `llm::process_text` unconditionally —
    /// translation is prompt-based, not config-threaded, post-Workflow-P2).
    /// Kept for that one-time migration read and for `SavedScenario`'s
    /// dictation-section snapshot/restore (and deserializing configs saved
    /// before this change); do not add new readers.
    pub translate_target: String,

    // ── Agent settings ────────────────────────────────────────────────────

    /// Which model profile ID to use for agent LLM calls (independent from `llm_profile`).
    /// Empty string means "fall back to `llm_profile`".
    pub agent_llm_profile: String,
    /// DEPRECATED: system prompt formerly injected into every agent LLM
    /// request. Superseded by the per-widget `AgentProps::system` fallback
    /// (Workbench P2 Task 6 Fix Round 1 — mirrors `DialogProps`'s inline
    /// `system` field): `migrate_legacy_agent_triggers` copies a non-empty
    /// value here into the `agent.default` widget's `props.system` once.
    ///
    /// That "no live readers left" framing was **false** until the final
    /// review wave's I1 fix: `main.rs` cached this field ONCE at startup into
    /// `commands::agent::AgentState.system_prompt`, and
    /// `commands::agent::agent_process` (the shared agent panel's typed/mic
    /// path) read that stale cache directly — so this field kept mattering,
    /// silently, well past migration, for any user who edited the widget's
    /// persona without restarting. I1 removed the cache entirely:
    /// `agent_process` now resolves its persona the same way the widget
    /// (voice) path does, via `commands::agent_widget::resolve_agent_default_persona`.
    /// This field is now genuinely reader-free beyond the one-time migration
    /// copy and deserializing configs saved before that migration ran; do not
    /// add new readers.
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
    ///
    /// Audited alongside the legacy `modes` system's deletion (Workbench P2
    /// Task 12) — unlike that task's other global settings fields, this one
    /// is genuinely still live-read: `injection::resolve_strategy_for_app`
    /// (used by the raw-dictation injection path) falls back to this field
    /// when no per-app override matches. A widget-level default (`out.insert`'s
    /// own `strategy` prop, resolved via `resolve_strategy_for_app_with_default`)
    /// is a separate, narrower fallback tier for the engine's insert output —
    /// this field stays the process-wide default beneath both.
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
    /// text. No longer read — the desktop shell's Listen command always
    /// resolves the built-in `llm.listen` widget via
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
    /// from `global_vocab_books` and per-widget `LlmProps.vocab_books` /
    /// `SttProps.vocab_books`.
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

    /// HuggingFace endpoint override for diarization model downloads
    /// (empty = official). E.g. "https://hf-mirror.com".
    #[serde(default)]
    pub diarization_hf_endpoint: String,

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

// ── Floating indicator idle shapes ──────────────────────────────────────────
//
// The four legal values of [`AppConfig::float_indicator`]. Named constants
// rather than an enum for the reason documented on that field (a bad value
// must not be able to reset the rest of the config); these keep the string
// literals from being retyped at each of the ~6 use sites across the desktop
// crate and the front end.

/// A bare 44×3 rule — no glyph, no wordmark. The smallest shape that still
/// reads as "the app is alive"; opt-in rather than the default, so an upgrade
/// never changes the indicator out from under someone.
pub const FLOAT_HAIRLINE: &str = "hairline";
/// A 14×14 waveform dot.
pub const FLOAT_DOT: &str = "dot";
/// The full pill with glyph and `FONOS` wordmark — the pre-0.8.7 look, and the
/// default: upgrading should not silently reshape an indicator people already
/// recognize. The smaller shapes are a setting away.
pub const FLOAT_PILL: &str = "pill";
/// No idle presence: hidden while there is nothing to report, but still shown
/// for the working states. See [`AppConfig::float_indicator`].
pub const FLOAT_OFF: &str = "off";

/// Resolve an arbitrary `float_indicator` string to one of the four legal
/// shapes, falling back to [`FLOAT_PILL`].
///
/// Pure and free-standing so both the config accessor and the desktop crate's
/// geometry table normalize identically — and so the fallback is unit-testable
/// without building an `AppConfig`.
pub fn normalize_float_indicator(raw: &str) -> &'static str {
    match raw {
        FLOAT_DOT => FLOAT_DOT,
        FLOAT_PILL => FLOAT_PILL,
        FLOAT_OFF => FLOAT_OFF,
        FLOAT_HAIRLINE => FLOAT_HAIRLINE,
        // The empty string, and anything a future version (or a hand-edited
        // config) might leave behind.
        _ => FLOAT_PILL,
    }
}

fn default_idle_fade_secs() -> u32 {
    8
}

/// Like [`lenient`], but falls back to [`default_idle_fade_secs`] rather than
/// `u32::default()`. Zero is a *meaningful* value here — it means "never dim" —
/// so a bad value must not quietly turn dormancy off; it should land on the
/// same 8 seconds a fresh install gets.
fn lenient_idle_fade_secs<'de, D>(d: D) -> std::result::Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_else(|_| default_idle_fade_secs()))
}

/// Deserialize a field, falling back to its default instead of failing the
/// whole struct when the value has the wrong shape.
///
/// [`AppConfig::load`] ends in `unwrap_or_default()`, so a single bad value
/// anywhere resets EVERY setting and the next save writes those defaults to
/// disk. That is a harsh penalty for a hand-edited `"float_indicator": null`.
/// Routing through `Value` first contains the damage to the one field.
///
/// Only applied to the float-indicator fields; retrofitting the rest of the
/// struct is a separate, larger change.
fn lenient<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

/// One-time migration of the legacy `show_floating_indicator` bool into
/// [`AppConfig::float_indicator`].
///
/// Returns `Some("off")` only when the on-disk JSON predates `float_indicator`
/// *and* had the indicator explicitly switched off — that is the single piece
/// of user intent the old bool ever carried. Every other case returns `None`,
/// meaning "leave the deserialized value alone":
/// - new key present → the user has already expressed a shape; the old bool is
///   stale and must not override it.
/// - old bool `true` or absent → wanted an indicator, and the new default
///   (the pill) is one.
///
/// Takes the two decisions as plain arguments rather than a `serde_json::Value`
/// so the policy is testable without constructing JSON.
fn migrated_float_indicator(has_new_key: bool, legacy_show: Option<bool>) -> Option<&'static str> {
    if has_new_key {
        return None;
    }
    match legacy_show {
        Some(false) => Some(FLOAT_OFF),
        _ => None,
    }
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
            float_indicator: FLOAT_PILL.to_string(),
            float_idle_fade_secs: 8,
            float_anchor_x: None,
            float_anchor_y: None,
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
            diarization_hf_endpoint: String::new(),
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
            Ok(json) => match serde_json::from_str::<Self>(&json) {
                Ok(mut cfg) => {
                    cfg.apply_float_indicator_migration(&json);
                    cfg
                }
                // Falling back to defaults here silently discards every setting
                // the user ever changed, and the next save writes those defaults
                // over the file — so keep a copy first and say so loudly.
                //
                // `lenient` contains the common bad values (null, wrong type,
                // negative) field-by-field, but it cannot contain everything:
                // a numeric literal outside f64's range (`1e400`) fails inside
                // serde_json's own number parsing, before any field-level
                // handler runs.
                Err(e) => {
                    eprintln!("fonos: config.json could not be parsed ({e})");
                    let backup = path.with_extension("json.corrupt");
                    match std::fs::copy(&path, &backup) {
                        Ok(_) => eprintln!(
                            "fonos: previous config preserved at {} — falling back to defaults",
                            backup.display()
                        ),
                        Err(e) => eprintln!("fonos: could not back up config.json: {e}"),
                    }
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    /// Fold a pre-`float_indicator` config's `show_floating_indicator: false`
    /// into `float_indicator = "off"`.
    ///
    /// Needs the raw JSON, not just the deserialized struct: `#[serde(default)]`
    /// has already filled `float_indicator` with the default by this point, so
    /// "absent from disk" and "explicitly set to that same shape" are
    /// indistinguishable on the struct. Re-parsing to `Value` is the cheap way
    /// to recover that distinction, and it happens once per load.
    ///
    /// A malformed file cannot reach here with anything meaningful (the struct
    /// parse above would have fallen back to defaults), so a `Value` parse
    /// failure is simply left alone.
    fn apply_float_indicator_migration(&mut self, raw_json: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_json) else {
            return;
        };
        let has_new_key = v.get("float_indicator").is_some();
        let legacy_show = v.get("show_floating_indicator").and_then(|b| b.as_bool());
        if let Some(migrated) = migrated_float_indicator(has_new_key, legacy_show) {
            self.float_indicator = migrated.to_string();
        }
    }

    /// The indicator's idle shape, normalized to one of the four legal values.
    /// Every reader should go through this rather than the raw field, so a
    /// hand-edited or future-version config degrades to the default shape
    /// instead of rendering nothing.
    pub fn float_form(&self) -> &'static str {
        normalize_float_indicator(&self.float_indicator)
    }

    /// The remembered indicator anchor (center of the visible mark, logical
    /// points), or `None` when the user has never dragged it — in which case
    /// callers compute the default bottom-of-screen placement instead.
    ///
    /// A non-finite coordinate is rejected as well as a half-written pair: NaN
    /// would poison every comparison downstream and infinity would place the
    /// window nowhere, and neither can be clamped back into a monitor.
    pub fn float_anchor(&self) -> Option<(f64, f64)> {
        match (self.float_anchor_x, self.float_anchor_y) {
            (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some((x, y)),
            _ => None,
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

    /// True when any model profile advertises `cap` in its capabilities array.
    fn has_capability_profile(&self, cap: &str) -> bool {
        self.model_profiles.iter().any(|p| {
            p.get("capabilities")
                .and_then(|c| c.as_array())
                .map(|arr| arr.iter().any(|v| v.as_str() == Some(cap)))
                .unwrap_or(false)
        })
    }

    /// The single source of truth for "will dictation actually transcribe?" —
    /// delegates to [`crate::services::is_stt_effectively_configured`], which
    /// mirrors the runtime STT resolution in `commands/dictation.rs` exactly:
    /// the `stt.default` widget's `model_profile` (the `"apple-speech"` sentinel
    /// or an existing profile) wins over the global `stt_profile` and, being set,
    /// never falls back — so a dangling widget ref poisons even a valid global
    /// default. Unlike LLM/TTS below, capability tags are irrelevant here: only
    /// the assigned default is authoritative. Read by the tray STT row and the
    /// `stt_configured` command that backs the frontend first-run gate.
    pub fn is_stt_configured(&self) -> bool {
        crate::services::is_stt_effectively_configured(self)
    }

    /// Same semantics for the LLM role (tray 🧠 row / unlock notification).
    pub fn is_llm_configured(&self) -> bool {
        !self.llm_profile.trim().is_empty() || self.has_capability_profile("llm")
    }

    /// Same semantics for the TTS role (tray 🔊 row / unlock notification).
    pub fn is_tts_configured(&self) -> bool {
        !self.tts_profile.trim().is_empty() || self.has_capability_profile("tts")
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

    // ── Floating indicator ──────────────────────────────────────────────────

    #[test]
    fn float_indicator_defaults_to_pill() {
        // The familiar pre-0.8.7 shape: an upgrade must not reshape the
        // indicator without being asked. Hairline/dot are opt-in.
        assert_eq!(AppConfig::default().float_form(), FLOAT_PILL);
        assert_eq!(AppConfig::default().float_idle_fade_secs, 8);
        assert_eq!(AppConfig::default().float_anchor(), None);
    }

    #[test]
    fn normalize_float_indicator_passes_through_legal_values() {
        for v in [FLOAT_HAIRLINE, FLOAT_DOT, FLOAT_PILL, FLOAT_OFF] {
            assert_eq!(normalize_float_indicator(v), v);
        }
    }

    #[test]
    fn normalize_float_indicator_falls_back_on_junk() {
        // A hand-edited config or a shape added by a newer version must
        // degrade to the default shape, never to "render nothing".
        for junk in ["", "HAIRLINE", "bar", "ribbon", "off "] {
            assert_eq!(normalize_float_indicator(junk), FLOAT_PILL);
        }
    }

    #[test]
    fn legacy_show_false_migrates_to_off() {
        assert_eq!(migrated_float_indicator(false, Some(false)), Some(FLOAT_OFF));
    }

    #[test]
    fn legacy_show_true_or_absent_keeps_the_new_default() {
        assert_eq!(migrated_float_indicator(false, Some(true)), None);
        assert_eq!(migrated_float_indicator(false, None), None);
    }

    #[test]
    fn new_key_always_wins_over_the_legacy_bool() {
        // Once the user has picked a shape, a stale `show_floating_indicator:
        // false` left in the same file must not silently hide the indicator.
        assert_eq!(migrated_float_indicator(true, Some(false)), None);
        assert_eq!(migrated_float_indicator(true, Some(true)), None);
        assert_eq!(migrated_float_indicator(true, None), None);
    }

    #[test]
    fn migration_applies_to_a_pre_float_indicator_config() {
        let raw = r#"{"show_floating_indicator": false}"#;
        let mut cfg: AppConfig = serde_json::from_str(raw).unwrap();
        // serde(default) has already filled in the pill default…
        assert_eq!(cfg.float_form(), FLOAT_PILL);
        cfg.apply_float_indicator_migration(raw);
        // …and the migration recovers the user's original intent.
        assert_eq!(cfg.float_form(), FLOAT_OFF);
    }

    #[test]
    fn migration_leaves_an_explicit_shape_untouched() {
        let raw = r#"{"show_floating_indicator": false, "float_indicator": "dot"}"#;
        let mut cfg: AppConfig = serde_json::from_str(raw).unwrap();
        cfg.apply_float_indicator_migration(raw);
        assert_eq!(cfg.float_form(), FLOAT_DOT);
    }

    #[test]
    fn float_anchor_requires_both_coordinates() {
        let mut cfg = AppConfig::default();
        cfg.float_anchor_x = Some(400.0);
        assert_eq!(cfg.float_anchor(), None, "half a pair is not a position");
        cfg.float_anchor_y = Some(900.0);
        assert_eq!(cfg.float_anchor(), Some((400.0, 900.0)));
    }

    #[test]
    fn float_anchor_rejects_non_finite_coordinates() {
        // NaN would poison every comparison downstream and infinity would place
        // the window nowhere; neither can be clamped back onto a monitor, so
        // both must read as "never dragged" and fall back to the default spot.
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut cfg = AppConfig::default();
            cfg.float_anchor_x = Some(bad);
            cfg.float_anchor_y = Some(900.0);
            assert_eq!(cfg.float_anchor(), None, "x = {bad}");
            cfg.float_anchor_x = Some(400.0);
            cfg.float_anchor_y = Some(bad);
            assert_eq!(cfg.float_anchor(), None, "y = {bad}");
        }
    }

    #[test]
    fn a_bad_fade_value_falls_back_to_the_field_default_not_zero() {
        // Zero is a MEANINGFUL value here — it means "never dim" — so a bad
        // value landing on `u32::default()` would silently disable dormancy
        // rather than behaving like a fresh install.
        for bad in [r#"{"float_idle_fade_secs": -1}"#, r#"{"float_idle_fade_secs": "soon"}"#, r#"{"float_idle_fade_secs": null}"#] {
            let cfg: AppConfig = serde_json::from_str(bad).unwrap();
            assert_eq!(cfg.float_idle_fade_secs, 8, "{bad}");
        }
        // An explicit 0 still means "never dim" and must survive.
        let cfg: AppConfig = serde_json::from_str(r#"{"float_idle_fade_secs": 0}"#).unwrap();
        assert_eq!(cfg.float_idle_fade_secs, 0);
    }

    #[test]
    fn a_bad_float_field_costs_only_that_field() {
        // `load()` ends in `unwrap_or_default()`, so without the lenient
        // deserializer any one of these would fail the whole struct and
        // silently reset every unrelated setting — which the next save would
        // then write to disk.
        for bad in [
            r#"{"float_indicator": null, "tts_speed": 1.75}"#,
            r#"{"float_indicator": 7, "tts_speed": 1.75}"#,
            r#"{"float_idle_fade_secs": -1, "tts_speed": 1.75}"#,
            r#"{"float_idle_fade_secs": "soon", "tts_speed": 1.75}"#,
            r#"{"float_anchor_x": "left", "float_anchor_y": 12, "tts_speed": 1.75}"#,
            r#"{"float_anchor_x": {}, "tts_speed": 1.75}"#,
        ] {
            let cfg: AppConfig = serde_json::from_str(bad)
                .unwrap_or_else(|e| panic!("{bad} should still parse, got {e}"));
            assert_eq!(cfg.tts_speed, 1.75, "unrelated setting lost for {bad}");
            assert_eq!(cfg.float_form(), FLOAT_PILL, "{bad}");
        }
    }

    #[test]
    fn unknown_float_indicator_does_not_reset_the_rest_of_the_config() {
        // The reason this field is a String and not an enum: `load()` ends in
        // `unwrap_or_default()`, so a value that failed to parse would take
        // every other setting down with it.
        let raw = r#"{"float_indicator": "some-future-shape", "tts_speed": 1.75}"#;
        let cfg: AppConfig = serde_json::from_str(raw).unwrap();
        assert_eq!(cfg.tts_speed, 1.75, "unrelated settings must survive");
        assert_eq!(cfg.float_form(), FLOAT_PILL);
    }

    #[test]
    fn migrate_transform_creates_binding_and_clears_legacy_hotkey() {
        let mut cfg = AppConfig::default(); // default hotkey_transform = "cmd+shift+t"
        assert!(migrate_transform_to_text_actions(&mut cfg));
        assert_eq!(cfg.text_actions.len(), 1);
        let b = &cfg.text_actions[0];
        assert_eq!(b.hotkey, "cmd+shift+t");
        assert_eq!(b.mode_id, "polish");
        assert_eq!(b.output_target, OutputTarget::ActiveTextField);
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
            output_target: OutputTarget::FloatingPopup,
        });
        assert!(!migrate_transform_to_text_actions(&mut existing));
        assert_eq!(existing.text_actions.len(), 1);
    }

    #[test]
    fn diarization_hf_endpoint_defaults_empty_and_roundtrips() {
        let c: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(c.diarization_hf_endpoint, "");
        let mut c2 = AppConfig::default();
        c2.diarization_hf_endpoint = "https://hf-mirror.com".into();
        let j = serde_json::to_string(&c2).unwrap();
        let c3: AppConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(c3.diarization_hf_endpoint, "https://hf-mirror.com");
    }

    #[test]
    fn capability_configured_checks() {
        let mut cfg = AppConfig::default();
        assert!(!cfg.is_stt_configured());
        assert!(!cfg.is_llm_configured());
        assert!(!cfg.is_tts_configured());

        // Direct profile assignment counts.
        cfg.llm_profile = "p1".to_string();
        assert!(cfg.is_llm_configured());

        // A capability-tagged profile counts for TTS even without a default
        // assignment (LLM/TTS keep the loose capability semantics).
        cfg.model_profiles = vec![serde_json::json!({
            "id": "p2", "name": "x", "provider": "openai",
            "model": "m", "capabilities": ["tts", "stt"]
        })];
        assert!(cfg.is_tts_configured());
        // STT no longer honors capability-only (Fix A: it mirrors the runtime,
        // which reads the assigned default, not the capabilities array). The
        // stt-capable profile above is unusable until it's the assigned default.
        assert!(!cfg.is_stt_configured());
        cfg.stt_profile = "p2".to_string();
        assert!(cfg.is_stt_configured());
        // A stt_profile pointing at a since-deleted profile is not configured.
        cfg.stt_profile = "ghost".to_string();
        assert!(!cfg.is_stt_configured());
        // Capabilities list without "llm" does not flip llm off its direct assignment.
        cfg.llm_profile = String::new();
        assert!(!cfg.is_llm_configured());
    }
}
