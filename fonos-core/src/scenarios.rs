//! Scenario-based setup — platform-independent model classification, role
//! assignment, and saved-configuration snapshot/apply logic (issue #29).
//!
//! This module holds the pure, testable half of the scenario picker:
//!
//! * **Classification** — [`classify_model`] / [`classify_models`] sort a
//!   server's `/v1/models` ids into STT / LLM / TTS candidates (or exclude
//!   embeddings, rerankers, and speech-enhancement nets) using name heuristics.
//! * **Role assignment** — [`pick_llm_candidate`] and [`assign_tts_roles`]
//!   turn the classified lists (plus measured TTS real-time factors) into a
//!   concrete [`ModelPlan`] of role → model. The RTF measurement itself needs
//!   network access and lives in the desktop shell (`commands::scenarios`).
//! * **Saved scenarios** — [`SavedScenario`] is a self-contained snapshot of
//!   the model profiles referenced by the current defaults plus those default
//!   assignments. [`snapshot_current`] captures the live config;
//!   [`apply_saved`] upserts a snapshot back in (never deleting unrelated
//!   profiles); [`parse_saved_scenario`] validates an imported JSON file.
//!
//! Everything here is free of Tauri / OS / network dependencies so it can be
//! unit-tested directly.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::workflow::migrate::LegacyMode;
use crate::workflow::model::{WidgetDef, WorkflowDef};

// ── model classification ────────────────────────────────────────────────────

/// Which pipeline role a model id is a candidate for, by name heuristics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelRole {
    /// Speech-to-text (recognition).
    Stt,
    /// Large language model (chat / text processing).
    Llm,
    /// Text-to-speech (synthesis).
    Tts,
    /// Not a usable dictation model — embeddings, rerankers, speech-enhancement
    /// / separation networks. Dropped from every candidate list.
    Exclude,
}

/// Classify a single `/v1/models` id into a [`ModelRole`] by case-insensitive
/// name heuristics.
///
/// Order matters: excludes are checked first (so a speech-enhancement net never
/// masquerades as an LLM), then STT, then TTS; anything left is an LLM
/// candidate.
pub fn classify_model(id: &str) -> ModelRole {
    let lower = id.to_lowercase();

    // Exclude: embeddings, rerankers, speech-enhancement / separation nets.
    const EXCLUDE: [&str; 5] = ["embed", "rerank", "deepfilter", "mossformer", "lfm2.5-audio"];
    if EXCLUDE.iter().any(|k| lower.contains(k)) {
        return ModelRole::Exclude;
    }
    // "SE" (speech enhancement) as a standalone token, e.g. "ZipEnhancer-SE".
    if lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == "se") {
        return ModelRole::Exclude;
    }

    // STT: ASR / Whisper / Paraformer / Parakeet families.
    const STT: [&str; 4] = ["asr", "whisper", "paraformer", "parakeet"];
    if STT.iter().any(|k| lower.contains(k)) {
        return ModelRole::Stt;
    }

    // TTS: common open synthesis families.
    const TTS: [&str; 6] = ["tts", "kokoro", "chatterbox", "vibevoice", "f5", "cosyvoice"];
    if TTS.iter().any(|k| lower.contains(k)) {
        return ModelRole::Tts;
    }

    ModelRole::Llm
}

/// STT / LLM / TTS candidate lists produced by [`classify_models`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClassifiedModels {
    /// Speech-to-text candidate model ids, in server order.
    pub stt: Vec<String>,
    /// LLM candidate model ids, in server order.
    pub llm: Vec<String>,
    /// Text-to-speech candidate model ids, in server order.
    pub tts: Vec<String>,
}

/// Classify a whole model list into STT / LLM / TTS candidate buckets, dropping
/// excluded models. Order within each bucket follows the input order.
pub fn classify_models(ids: &[String]) -> ClassifiedModels {
    let mut out = ClassifiedModels::default();
    for id in ids {
        match classify_model(id) {
            ModelRole::Stt => out.stt.push(id.clone()),
            ModelRole::Llm => out.llm.push(id.clone()),
            ModelRole::Tts => out.tts.push(id.clone()),
            ModelRole::Exclude => {}
        }
    }
    out
}

// ── parameter-size parsing ──────────────────────────────────────────────────

/// Best-effort parse of a model's parameter count in **billions** from its id,
/// reading tokens like `70b`, `7B`, `1.7b`, or `82M` (megaparams → billions).
///
/// Returns the largest such token found, or `0.0` when the id carries no size.
pub fn param_size_billions(id: &str) -> f64 {
    let lower = id.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut best = 0.0_f64;
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            if i < chars.len() && (chars[i] == 'b' || chars[i] == 'm') {
                let suffix = chars[i];
                // Require a token boundary after the suffix so "base" isn't a "b".
                let boundary = i + 1 >= chars.len() || !chars[i + 1].is_ascii_alphanumeric();
                if boundary {
                    let num_str: String = chars[start..i].iter().collect();
                    if let Ok(num) = num_str.parse::<f64>() {
                        let billions = if suffix == 'm' { num / 1000.0 } else { num };
                        if billions > best {
                            best = billions;
                        }
                    }
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    best
}

// ── LLM selection ───────────────────────────────────────────────────────────

/// Heuristic "goodness" score for an LLM candidate: parameter size, with a
/// large bonus for instruction/chat-tuned names.
fn llm_score(id: &str) -> f64 {
    let lower = id.to_lowercase();
    let mut score = param_size_billions(&lower);
    if lower.contains("instruct") || lower.contains("chat") || lower.contains("-it") {
        score += 1000.0;
    }
    score
}

/// Pick the best default LLM from `candidates`, preferring instruct/chat names
/// and larger parameter counts. Ties keep the first (server-order) candidate.
pub fn pick_llm_candidate(candidates: &[String]) -> Option<String> {
    let mut best: Option<(&String, f64)> = None;
    for c in candidates {
        let s = llm_score(c);
        match best {
            Some((_, bs)) if s <= bs => {}
            _ => best = Some((c, s)),
        }
    }
    best.map(|(c, _)| c.clone())
}

// ── TTS role assignment ─────────────────────────────────────────────────────

/// Heuristic "quality" score for a TTS candidate: parameter size, with a large
/// bonus for the high-fidelity `qwen3-tts` family.
fn tts_quality_score(id: &str) -> f64 {
    let lower = id.to_lowercase();
    let mut s = param_size_billions(&lower);
    if lower.contains("qwen3-tts") || lower.contains("qwen3_tts") {
        s += 1000.0;
    }
    s
}

/// The two spoken-voice roles a scenario assigns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TtsRoles {
    /// Fast, low-latency voice for back-and-forth conversation (RTF &lt; 1 preferred).
    pub conversation: Option<String>,
    /// Highest-quality voice for long-form listening / reading aloud.
    pub listen: Option<String>,
}

/// Assign the conversation and listen voices from the TTS candidates.
///
/// * No TTS → both roles unassigned.
/// * Exactly one TTS → it takes both roles.
/// * Several → the lowest-RTF model (fastest, real-time preferred) becomes the
///   conversation voice; the highest-quality model becomes the listen voice.
///   When those collide, the listen voice falls back to the next-best-quality
///   model that isn't the conversation voice, so the two roles differ whenever
///   possible.
///
/// `rtfs` maps a model id to its measured real-time factor; models absent from
/// the map are treated as slowest (never chosen as fastest on their own).
pub fn assign_tts_roles(tts_models: &[String], rtfs: &BTreeMap<String, f64>) -> TtsRoles {
    match tts_models.len() {
        0 => return TtsRoles::default(),
        1 => {
            return TtsRoles {
                conversation: Some(tts_models[0].clone()),
                listen: Some(tts_models[0].clone()),
            }
        }
        _ => {}
    }

    let rtf_of = |m: &str| rtfs.get(m).copied().unwrap_or(f64::INFINITY);

    // Conversation: strictly-lowest RTF (first wins on ties).
    let mut fastest = &tts_models[0];
    for m in &tts_models[1..] {
        if rtf_of(m) < rtf_of(fastest) {
            fastest = m;
        }
    }
    let conversation = fastest.clone();

    // Listen: highest quality, preferring a model that isn't the conversation one.
    let mut best_q = &tts_models[0];
    for m in &tts_models[1..] {
        if tts_quality_score(m) > tts_quality_score(best_q) {
            best_q = m;
        }
    }
    let listen = if *best_q == conversation {
        let mut alt: Option<&String> = None;
        for m in tts_models {
            if *m == conversation {
                continue;
            }
            match alt {
                Some(a) if tts_quality_score(m) <= tts_quality_score(a) => {}
                _ => alt = Some(m),
            }
        }
        alt.cloned().unwrap_or_else(|| best_q.clone())
    } else {
        best_q.clone()
    };

    TtsRoles {
        conversation: Some(conversation),
        listen: Some(listen),
    }
}

/// A concrete role → model assignment produced from classified candidates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelPlan {
    /// Chosen speech-to-text model (first STT candidate), if any.
    pub stt: Option<String>,
    /// Chosen LLM model, if any.
    pub llm: Option<String>,
    /// Chosen conversation (fast) voice model, if any.
    pub conversation_tts: Option<String>,
    /// Chosen listen (high-quality) voice model, if any.
    pub listen_tts: Option<String>,
}

/// Build a default [`ModelPlan`] from classified candidates and measured TTS RTFs.
pub fn build_plan(classified: &ClassifiedModels, tts_rtfs: &BTreeMap<String, f64>) -> ModelPlan {
    let roles = assign_tts_roles(&classified.tts, tts_rtfs);
    ModelPlan {
        stt: classified.stt.first().cloned(),
        llm: pick_llm_candidate(&classified.llm),
        conversation_tts: roles.conversation,
        listen_tts: roles.listen,
    }
}

// ── existing-profile reuse ──────────────────────────────────────────────────

/// Trim a trailing slash for base-URL comparison.
fn norm_url(u: &str) -> &str {
    u.trim_end_matches('/')
}

/// Find an existing model profile whose base URL and model both match, returning
/// its id. Used so applying a scenario reuses an identical profile instead of
/// creating a duplicate. Never matches on an empty model.
pub fn find_matching_profile(
    profiles: &[serde_json::Value],
    base_url: &str,
    model: &str,
) -> Option<String> {
    if model.is_empty() {
        return None;
    }
    let target = norm_url(base_url);
    profiles.iter().find_map(|p| {
        let p_url = norm_url(p["base_url"].as_str().unwrap_or(""));
        let p_model = p["model"].as_str().unwrap_or("");
        if p_url == target && p_model == model {
            p["id"].as_str().map(|s| s.to_string())
        } else {
            None
        }
    })
}

// ── saved scenarios (snapshot / apply / import) ─────────────────────────────

/// The default-service assignments captured by a [`SavedScenario`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenarioAssignments {
    /// Default speech-to-text profile id.
    pub stt_profile: String,
    /// Default LLM profile id.
    pub llm_profile: String,
    /// Default text-to-speech profile id.
    pub tts_profile: String,
    /// Conversation (spoken reply) voice profile id.
    pub sts_voice_profile: String,
    /// Listen (read-aloud) voice profile id.
    pub listen_voice_profile: String,
    /// Conversation voice identifier.
    pub sts_voice: String,
    /// Listen voice identifier.
    pub listen_voice: String,
}

/// The **models** section of a saved scenario: the model profiles referenced by
/// the default assignments, plus those assignments. Present when a save includes
/// the model/role configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsSection {
    /// Snapshot of the model-profile JSON entries the assignments reference.
    pub profiles: Vec<serde_json::Value>,
    /// The default-service assignments to restore.
    pub assignments: ScenarioAssignments,
}

/// The **dictation** section of a saved scenario: the user's workflow-engine
/// overlays plus the two config fields that drive dictation behaviour.
///
/// Evolved from a `modes.json`-shaped snapshot into a workflows/widgets-shaped
/// one (Workbench P2 Task 11 — the engine world superseded modes back in
/// Workflow P1, but the scenario snapshot only caught up then; Task 12 has
/// since deleted `modes.rs` entirely):
///
/// * `user_workflows` / `user_widgets` — `config.workflows` / `config.widgets`
///   verbatim (the overlay list: custom entries plus built-in overrides,
///   exactly the shape [`crate::workflow::engine::effective_workflows`] /
///   [`effective_widgets`](crate::workflow::engine::effective_widgets)
///   overlay onto the built-ins). Populated by every snapshot from this task
///   onward; applied by upserting each entry into the live config by id
///   (never deleting unrelated ones — same convention as the `models`
///   section's profile upsert).
/// * `user_modes` — DEPRECATED: the legacy `modes.json` map (id → mode,
///   deserialized via [`crate::workflow::migrate::LegacyMode`]),
///   present only in scenarios saved *before* this task. New snapshots always
///   leave it `null` — [`snapshot_current`] no longer reads `modes.json` at
///   all. [`apply_saved`] still recognizes a non-null value here and converts
///   its content-bearing custom modes into `llm.*` processor widgets via the
///   same conversion [`crate::workflow::migrate::migrate_to_workflows`] uses
///   (a scratch config, so the live config's already-migrated workflows/
///   triggers are never touched), rather than writing them back to
///   `modes.json` (which T12 deletes). Kept only for that one-time import
///   read and to deserialize pre-Task-11 scenario files without error.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationSection {
    /// DEPRECATED — see struct doc. Import-compat only.
    pub user_modes: serde_json::Value,
    /// User workflow overlays (`config.workflows` verbatim).
    #[serde(default)]
    pub user_workflows: Vec<WorkflowDef>,
    /// User widget overlays (`config.widgets` verbatim).
    #[serde(default)]
    pub user_widgets: Vec<WidgetDef>,
    /// Default dictation mode id.
    pub dictation_mode: String,
    /// Target language for translation mode.
    pub translate_target: String,
}

/// The **speech** section of a saved scenario: the Listen queue + STS
/// conversation configuration (voice output pipelines).
///
/// Audited for Workbench P2 Task 11 against each field's actual live readers
/// (not just its config.rs doc-comment claim, which for a couple of these
/// fields turned out to be stale): `listen_mode` and `sts_persona` /
/// `sts_max_turns` have no reader anywhere in the app anymore and are no
/// longer snapshotted. `sts_llm_profile` remains — it's the `call.default`
/// composite's documented live-read fallback rung.
///
/// `sts_voice_profile` / `sts_voice` were dropped in Workbench P2 Task 14:
/// Task 11's audit kept them here because `commands::doctor::
/// check_conversation_rtf` still read them directly, but Task 14 repointed
/// that probe at `call.default`'s own `CallProps` (mirroring
/// `resolve_call_cfg`'s TTS branch), so these two config fields are now
/// zero-reader outside the one-time migration that seeded `call.default`
/// from them — restoring them via a scenario would no longer do anything.
/// (`ScenarioAssignments.sts_voice_profile`/`.sts_voice`, the **models**
/// section's copies, are a separate concern — untouched.)
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechSection {
    /// TTS profile id for Listen synthesis.
    pub listen_voice_profile: String,
    /// Voice identifier for Listen synthesis.
    pub listen_voice: String,
    /// LLM profile id for conversation.
    pub sts_llm_profile: String,
}

/// The **vocab** section of a saved scenario: the user's custom vocabulary
/// books plus the globally-applied book ids. Pure config fields — applied by
/// overwriting `vocab_books` / `global_vocab_books`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VocabSection {
    /// User-defined vocab books, captured verbatim.
    pub vocab_books: Vec<crate::vocab::VocabBook>,
    /// Book ids applied to every dictation regardless of mode.
    pub global_vocab_books: Vec<String>,
}

/// The **hotkeys** section of a saved scenario: every global-hotkey binding plus
/// the three notebook-shortcut bindings. Pure config fields; applying them takes
/// effect live once the hotkey manager re-registers (the shell emits the reload).
///
/// Audited for Workbench P2 Task 11 (the "T6 mandate": old scenarios must not
/// write back dead hotkey fields): `hotkey_agent` / `hotkey_agent_panel`
/// (Task 6), `hotkey_meeting` (Task 7), and `hotkey_sts` (Task 9) are each
/// one-time-folded into a `Trigger::Hotkey` chip on the matching recipe and
/// then cleared, with no reader left anywhere — restoring them from an old
/// scenario would silently resurrect a field nothing dispatches on anymore.
/// Dropped from the section entirely rather than merely left unapplied, so a
/// pre-Task-11 scenario's stale values are also ignored on import (unknown
/// JSON keys deserialize away quietly). `hotkey_transform` stays: unlike the
/// others it still drives a real reconciliation
/// ([`crate::config::migrate_transform_to_text_actions`]) that [`apply_saved`]
/// runs on every hotkeys-section apply.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysSection {
    /// Hold-to-talk dictation combo.
    pub hotkey_dictation: String,
    /// Toggle dictation combo (press to start / stop).
    pub hotkey_dictation_toggle: String,
    /// TTS playback combo.
    pub hotkey_tts: String,
    /// Toggle note panel combo.
    pub hotkey_note: String,
    /// Notebook shortcut 1 combo.
    pub hotkey_note_1: String,
    /// Notebook shortcut 2 combo.
    pub hotkey_note_2: String,
    /// Notebook shortcut 3 combo.
    pub hotkey_note_3: String,
    /// Container id bound to notebook shortcut 1 (0 = unbound).
    pub notebook_hotkey_1: i64,
    /// Container id bound to notebook shortcut 2 (0 = unbound).
    pub notebook_hotkey_2: i64,
    /// Container id bound to notebook shortcut 3 (0 = unbound).
    pub notebook_hotkey_3: i64,
    /// Quick-transform combo.
    pub hotkey_transform: String,
    /// Capture-into-Listen-queue combo.
    pub hotkey_listen: String,
    /// Text-action bindings snapshot. `None` = scenario predates text actions
    /// (apply leaves the live config's bindings untouched); `Some` = apply verbatim.
    #[serde(default)]
    pub text_actions: Option<Vec<crate::config::TextActionBinding>>,
}

/// A self-contained, shareable snapshot of a working configuration. Evolved from
/// a flat models-only bundle into a **sectioned** bundle: each of models /
/// dictation / speech / vocab / hotkeys is an independent, optional section, so a
/// save can carry any subset and an apply restores only the sections present.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedScenario {
    /// Stable unique id.
    pub id: String,
    /// Human-friendly name.
    pub name: String,
    /// Creation time as a unix-epoch-seconds string (formatted by the frontend).
    pub created_at: String,
    /// Model profiles + role assignments, when the save includes models.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<ModelsSection>,
    /// Custom modes + dictation config fields, when the save includes dictation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dictation: Option<DictationSection>,
    /// Listen + STS conversation config, when the save includes speech.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech: Option<SpeechSection>,
    /// Custom vocabulary books + global book ids, when the save includes vocab.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vocab: Option<VocabSection>,
    /// Global + notebook hotkey bindings, when the save includes hotkeys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotkeys: Option<HotkeysSection>,
}

impl SavedScenario {
    /// The section badges this scenario carries, in fixed order: any of
    /// `"models"`, `"dictation"`, `"speech"`, `"vocab"`, `"hotkeys"` whose
    /// section is present.
    pub fn sections(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.models.is_some() {
            out.push("models");
        }
        if self.dictation.is_some() {
            out.push("dictation");
        }
        if self.speech.is_some() {
            out.push("speech");
        }
        if self.vocab.is_some() {
            out.push("vocab");
        }
        if self.hotkeys.is_some() {
            out.push("hotkeys");
        }
        out
    }
}

/// Milliseconds since the unix epoch (monotonic-enough id seed).
fn epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Seconds since the unix epoch, as a string (the frontend renders relative time).
fn epoch_secs_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

/// Lowercase-alphanumeric slug of a name, dashes for runs of other characters.
fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let s = out.trim_matches('-').to_string();
    if s.is_empty() {
        "scenario".to_string()
    } else {
        s
    }
}

/// Generate a fresh scenario id from a name and the current time.
pub fn generate_scenario_id(name: &str) -> String {
    format!("saved-{}-{}", slug(name), epoch_millis())
}

/// Public helper so shells can produce a shareable slug for filenames.
pub fn scenario_slug(name: &str) -> String {
    slug(name)
}

/// Capture the model profiles referenced by the current default assignments
/// (STT / LLM / TTS / conversation / listen), de-duplicated in reference order.
fn referenced_profiles(config: &AppConfig) -> Vec<serde_json::Value> {
    let referenced = [
        config.stt_profile.trim(),
        config.llm_profile.trim(),
        config.tts_profile.trim(),
        config.sts_voice_profile.trim(),
        config.listen_voice_profile.trim(),
    ];
    let mut seen: Vec<String> = Vec::new();
    let mut profiles: Vec<serde_json::Value> = Vec::new();
    for id in referenced {
        if id.is_empty() || seen.iter().any(|s| s == id) {
            continue;
        }
        if let Some(p) = config
            .model_profiles
            .iter()
            .find(|p| p["id"].as_str() == Some(id))
        {
            seen.push(id.to_string());
            profiles.push(p.clone());
        }
    }
    profiles
}

/// Snapshot the live config into a sectioned [`SavedScenario`] named `name`,
/// capturing only the sections whose `include_*` flag is set.
///
/// * **models** — the referenced model profiles + the default role assignments.
/// * **dictation** — `config.workflows` / `config.widgets` verbatim (the
///   user's engine-world overlays) plus the dictation-mode / translate-target
///   config fields. `user_modes` is always left `null` — modes.json hasn't
///   been the source of truth since Workflow P1 (Workbench P2 Task 11).
/// * **speech** — the Listen + STS conversation configuration.
/// * **vocab** — the custom vocabulary books + globally-applied book ids.
/// * **hotkeys** — every global + notebook hotkey binding.
pub fn snapshot_current(
    config: &AppConfig,
    name: &str,
    include_models: bool,
    include_dictation: bool,
    include_speech: bool,
    include_vocab: bool,
    include_hotkeys: bool,
) -> SavedScenario {
    let models = include_models.then(|| ModelsSection {
        profiles: referenced_profiles(config),
        assignments: ScenarioAssignments {
            stt_profile: config.stt_profile.clone(),
            llm_profile: config.llm_profile.clone(),
            tts_profile: config.tts_profile.clone(),
            sts_voice_profile: config.sts_voice_profile.clone(),
            listen_voice_profile: config.listen_voice_profile.clone(),
            sts_voice: config.sts_voice.clone(),
            listen_voice: config.listen_voice.clone(),
        },
    });
    let dictation = include_dictation.then(|| DictationSection {
        user_modes: serde_json::Value::Null,
        user_workflows: config.workflows.clone(),
        user_widgets: config.widgets.clone(),
        dictation_mode: config.dictation_mode.clone(),
        translate_target: config.translate_target.clone(),
    });
    let speech = include_speech.then(|| SpeechSection {
        listen_voice_profile: config.listen_voice_profile.clone(),
        listen_voice: config.listen_voice.clone(),
        sts_llm_profile: config.sts_llm_profile.clone(),
    });
    let vocab = include_vocab.then(|| VocabSection {
        vocab_books: config.vocab_books.clone(),
        global_vocab_books: config.global_vocab_books.clone(),
    });
    let hotkeys = include_hotkeys.then(|| HotkeysSection {
        hotkey_dictation: config.hotkey_dictation.clone(),
        hotkey_dictation_toggle: config.hotkey_dictation_toggle.clone(),
        hotkey_tts: config.hotkey_tts.clone(),
        hotkey_note: config.hotkey_note.clone(),
        hotkey_note_1: config.hotkey_note_1.clone(),
        hotkey_note_2: config.hotkey_note_2.clone(),
        hotkey_note_3: config.hotkey_note_3.clone(),
        notebook_hotkey_1: config.notebook_hotkey_1,
        notebook_hotkey_2: config.notebook_hotkey_2,
        notebook_hotkey_3: config.notebook_hotkey_3,
        hotkey_transform: config.hotkey_transform.clone(),
        hotkey_listen: config.hotkey_listen.clone(),
        text_actions: Some(config.text_actions.clone()),
    });
    SavedScenario {
        id: generate_scenario_id(name),
        name: name.to_string(),
        created_at: epoch_secs_string(),
        models,
        dictation,
        speech,
        vocab,
        hotkeys,
    }
}

/// Insert `workflow` into `config.workflows`, replacing any existing entry
/// with the same id — same upsert convention as the `models` section's
/// profile restore (never deletes unrelated entries).
fn upsert_workflow_overlay(config: &mut AppConfig, workflow: WorkflowDef) {
    match config.workflows.iter_mut().find(|w| w.id == workflow.id) {
        Some(slot) => *slot = workflow,
        None => config.workflows.push(workflow),
    }
}

/// Insert `widget` into `config.widgets`, replacing any existing entry with
/// the same id — see [`upsert_workflow_overlay`].
fn upsert_widget_overlay(config: &mut AppConfig, widget: WidgetDef) {
    match config.widgets.iter_mut().find(|w| w.id == widget.id) {
        Some(slot) => *slot = widget,
        None => config.widgets.push(widget),
    }
}

/// Convert a pre-Task-11 scenario's legacy `user_modes` map into the `llm.*`
/// processor widgets its content-bearing custom modes would have become,
/// using the exact same rule
/// [`crate::workflow::migrate::migrate_to_workflows`]'s rule 1 applies to
/// `modes.json` at startup — but run against a throwaway, freshly-defaulted
/// scratch config rather than the live one.
///
/// This matters: the live config has long since finished its own one-time
/// migration (`workflow_migration_done` is set), and `migrate_to_workflows`
/// is a whole-config migration — rules 2/4/5 rebuild `wf.dictation` /
/// `wf.note*` / `wf.listen` / `wf.ta-*` from the live legacy hotkey/mode
/// fields (mostly blanked by now) and would clobber their already-migrated
/// `triggers` (Hotkey/PillSlot chips added by later migrations), which
/// `upsert_workflow` replaces wholesale. Running the real function against an
/// isolated scratch config sidesteps that entirely: only rule 1 can produce
/// anything (the scratch config's dictation/listen/text-action fields are all
/// at their neutral defaults, so rules 2/4/5 never reference a custom mode),
/// and only its `llm.{id}` widget outputs are pulled back out.
fn widgets_from_legacy_modes(custom_modes: &BTreeMap<String, LegacyMode>) -> Vec<WidgetDef> {
    let mut scratch = AppConfig { workflow_migration_done: false, ..Default::default() };
    crate::workflow::migrate::migrate_to_workflows(&mut scratch, custom_modes);
    custom_modes
        .keys()
        .filter_map(|mode_id| {
            let widget_id = format!("llm.{mode_id}");
            scratch.widgets.iter().find(|w| w.id == widget_id).cloned()
        })
        .collect()
}

/// Apply a [`SavedScenario`] to `config`, restoring **only the sections that are
/// present** (a section-selective apply):
///
/// * **models** — upsert the snapshot profiles by id (replacing a same-id
///   profile, else appending — **never** deleting unrelated profiles), then
///   restore the role assignments.
/// * **speech** — set the Listen + STS conversation fields.
/// * **vocab** — overwrite the vocabulary books + global book ids.
/// * **hotkeys** — overwrite every global + notebook hotkey binding (the shell
///   re-registers them after this returns). Text-action bindings are restored
///   verbatim when the snapshot carries them (`Some`); when it predates text
///   actions (`None`), the live bindings are left untouched. Either way the
///   legacy `hotkey_transform` combo is reconciled afterward — migrated into a
///   binding if the binding list is empty, else cleared — so it can never
///   survive as a dead key that nothing registers.
/// * **dictation** — set the dictation-mode / translate-target config fields;
///   upsert `user_workflows` / `user_widgets` into `config.workflows` /
///   `config.widgets` by id (never deleting unrelated overlays — same
///   convention as `models`). A non-null legacy `user_modes` (a pre-Task-11
///   scenario) is additionally converted into `llm.*` processor widgets via
///   [`widgets_from_legacy_modes`] and upserted the same way, so an old
///   scenario's custom modes still land somewhere real instead of being
///   silently dropped, without ever touching `modes.json`.
pub fn apply_saved(config: &mut AppConfig, scenario: &SavedScenario) {
    if let Some(m) = &scenario.models {
        for p in &m.profiles {
            let pid = p["id"].as_str().unwrap_or("");
            if pid.is_empty() {
                continue;
            }
            if let Some(existing) = config
                .model_profiles
                .iter_mut()
                .find(|e| e["id"].as_str() == Some(pid))
            {
                *existing = p.clone();
            } else {
                config.model_profiles.push(p.clone());
            }
        }
        let a = &m.assignments;
        config.stt_profile = a.stt_profile.clone();
        config.llm_profile = a.llm_profile.clone();
        config.tts_profile = a.tts_profile.clone();
        config.sts_voice_profile = a.sts_voice_profile.clone();
        config.listen_voice_profile = a.listen_voice_profile.clone();
        if !a.sts_voice.trim().is_empty() {
            config.sts_voice = a.sts_voice.clone();
        }
        if !a.listen_voice.trim().is_empty() {
            config.listen_voice = a.listen_voice.clone();
        }
    }

    if let Some(s) = &scenario.speech {
        config.listen_voice_profile = s.listen_voice_profile.clone();
        config.sts_llm_profile = s.sts_llm_profile.clone();
        if !s.listen_voice.trim().is_empty() {
            config.listen_voice = s.listen_voice.clone();
        }
    }

    if let Some(v) = &scenario.vocab {
        config.vocab_books = v.vocab_books.clone();
        config.global_vocab_books = v.global_vocab_books.clone();
    }

    if let Some(h) = &scenario.hotkeys {
        config.hotkey_dictation = h.hotkey_dictation.clone();
        config.hotkey_dictation_toggle = h.hotkey_dictation_toggle.clone();
        config.hotkey_tts = h.hotkey_tts.clone();
        config.hotkey_note = h.hotkey_note.clone();
        config.hotkey_note_1 = h.hotkey_note_1.clone();
        config.hotkey_note_2 = h.hotkey_note_2.clone();
        config.hotkey_note_3 = h.hotkey_note_3.clone();
        config.notebook_hotkey_1 = h.notebook_hotkey_1;
        config.notebook_hotkey_2 = h.notebook_hotkey_2;
        config.notebook_hotkey_3 = h.notebook_hotkey_3;
        config.hotkey_transform = h.hotkey_transform.clone();
        config.hotkey_listen = h.hotkey_listen.clone();

        if let Some(ref ta) = h.text_actions {
            config.text_actions = ta.clone();
        }
        // Reconcile the legacy field so applying a scenario can never leave a
        // dead "transform" combo: convert it (pre-migration scenario onto an
        // empty binding list) or clear it (bindings already present).
        if !crate::config::migrate_transform_to_text_actions(config)
            && !config.hotkey_transform.is_empty()
            && !config.text_actions.is_empty()
        {
            config.hotkey_transform.clear();
        }
    }

    if let Some(d) = &scenario.dictation {
        config.dictation_mode = d.dictation_mode.clone();
        config.translate_target = d.translate_target.clone();

        for wf in d.user_workflows.iter().cloned() {
            upsert_workflow_overlay(config, wf);
        }
        for w in d.user_widgets.iter().cloned() {
            upsert_widget_overlay(config, w);
        }

        if !d.user_modes.is_null() {
            if let Ok(custom_modes) = serde_json::from_value::<BTreeMap<String, LegacyMode>>(d.user_modes.clone())
            {
                for w in widgets_from_legacy_modes(&custom_modes) {
                    upsert_widget_overlay(config, w);
                }
            }
        }
    }
}

/// Parse and validate an imported scenario JSON string, returning it with a
/// **fresh id** (so imports never collide with existing saved scenarios).
///
/// Accepts both the current **sectioned** shape (at least one of `models` /
/// `dictation` / `speech` / `vocab` / `hotkeys`) and the legacy **flat** shape
/// (top-level `profiles` array + `assignments` object), which is migrated into a
/// `models` section. Any other JSON produces a clear error rather than a
/// silently-defaulted empty scenario.
pub fn parse_saved_scenario(json: &str) -> Result<SavedScenario, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    if !value.is_object() {
        return Err("not a fonos scenario file".to_string());
    }

    let has_section = ["models", "dictation", "speech", "vocab", "hotkeys"]
        .iter()
        .any(|k| value.get(*k).map_or(false, |v| !v.is_null()));

    if !has_section {
        // Migrate the legacy flat shape (top-level profiles + assignments).
        let old_shape = value.get("profiles").map_or(false, |p| p.is_array())
            && value.get("assignments").map_or(false, |a| a.is_object());
        if !old_shape {
            return Err("not a fonos scenario file".to_string());
        }
        let obj = value.as_object_mut().unwrap();
        let profiles = obj.remove("profiles").unwrap_or_else(|| serde_json::json!([]));
        let assignments = obj.remove("assignments").unwrap_or_else(|| serde_json::json!({}));
        obj.insert(
            "models".to_string(),
            serde_json::json!({ "profiles": profiles, "assignments": assignments }),
        );
    }

    let mut scenario: SavedScenario =
        serde_json::from_value(value).map_err(|e| format!("invalid scenario: {e}"))?;
    scenario.id = generate_scenario_id(&scenario.name);
    Ok(scenario)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn classify_covers_stt_tts_llm_and_excludes() {
        assert_eq!(classify_model("Qwen3-ASR-1.7B"), ModelRole::Stt);
        assert_eq!(classify_model("whisper-large-v3"), ModelRole::Stt);
        assert_eq!(classify_model("paraformer-zh"), ModelRole::Stt);
        assert_eq!(classify_model("Kokoro-82M"), ModelRole::Tts);
        assert_eq!(classify_model("gpt-4o-mini-tts"), ModelRole::Tts);
        assert_eq!(classify_model("CosyVoice2-0.5B"), ModelRole::Tts);
        assert_eq!(classify_model("Qwen3-4B-Instruct-2507"), ModelRole::Llm);
        assert_eq!(classify_model("bge-m3-embed"), ModelRole::Exclude);
        assert_eq!(classify_model("bge-reranker-v2"), ModelRole::Exclude);
        assert_eq!(classify_model("MossFormer2-SE-48K"), ModelRole::Exclude);
        assert_eq!(classify_model("ZipEnhancer-SE"), ModelRole::Exclude);
        assert_eq!(classify_model("LFM2.5-Audio-1.5B"), ModelRole::Exclude);
    }

    #[test]
    fn classify_models_buckets_and_drops_excludes() {
        let c = classify_models(&ids(&[
            "Qwen3-ASR-1.7B",
            "Qwen3-8B-Instruct",
            "Kokoro-82M",
            "Qwen3-TTS-1.7B",
            "bge-m3-embed",
        ]));
        assert_eq!(c.stt, ids(&["Qwen3-ASR-1.7B"]));
        assert_eq!(c.llm, ids(&["Qwen3-8B-Instruct"]));
        assert_eq!(c.tts, ids(&["Kokoro-82M", "Qwen3-TTS-1.7B"]));
    }

    #[test]
    fn param_size_parses_common_forms() {
        assert!((param_size_billions("Qwen3-70B") - 70.0).abs() < 1e-9);
        assert!((param_size_billions("llama-3.3-8b-instruct") - 8.0).abs() < 1e-9);
        assert!((param_size_billions("Qwen3-1.7B") - 1.7).abs() < 1e-9);
        assert!((param_size_billions("Kokoro-82M") - 0.082).abs() < 1e-9);
        assert_eq!(param_size_billions("gpt-4o-mini"), 0.0);
    }

    #[test]
    fn pick_llm_prefers_instruct_and_larger() {
        let picked = pick_llm_candidate(&ids(&[
            "Qwen3-4B-Instruct",
            "Qwen3-8B-Instruct",
            "some-base-32b",
        ]));
        assert_eq!(picked.as_deref(), Some("Qwen3-8B-Instruct"));
        // No candidates → None.
        assert_eq!(pick_llm_candidate(&[]), None);
    }

    #[test]
    fn assign_tts_none_and_single() {
        assert_eq!(assign_tts_roles(&[], &BTreeMap::new()), TtsRoles::default());
        let one = assign_tts_roles(&ids(&["Kokoro-82M"]), &BTreeMap::new());
        assert_eq!(one.conversation.as_deref(), Some("Kokoro-82M"));
        assert_eq!(one.listen.as_deref(), Some("Kokoro-82M"));
    }

    #[test]
    fn assign_tts_multiple_splits_fast_and_hq() {
        let models = ids(&["Kokoro-82M", "Qwen3-TTS-1.7B"]);
        let mut rtfs = BTreeMap::new();
        rtfs.insert("Kokoro-82M".to_string(), 0.5);
        rtfs.insert("Qwen3-TTS-1.7B".to_string(), 1.8);
        let roles = assign_tts_roles(&models, &rtfs);
        assert_eq!(roles.conversation.as_deref(), Some("Kokoro-82M")); // fastest
        assert_eq!(roles.listen.as_deref(), Some("Qwen3-TTS-1.7B")); // highest quality
    }

    #[test]
    fn assign_tts_rtf_tie_is_deterministic_first() {
        let models = ids(&["voice-a", "voice-b"]);
        let mut rtfs = BTreeMap::new();
        rtfs.insert("voice-a".to_string(), 0.7);
        rtfs.insert("voice-b".to_string(), 0.7);
        let roles = assign_tts_roles(&models, &rtfs);
        assert_eq!(roles.conversation.as_deref(), Some("voice-a"));
        // Same quality (no size, no qwen3-tts) → listen falls back to the other.
        assert_eq!(roles.listen.as_deref(), Some("voice-b"));
    }

    #[test]
    fn build_plan_end_to_end() {
        let classified = classify_models(&ids(&[
            "Qwen3-ASR-1.7B",
            "Qwen3-8B-Instruct",
            "Kokoro-82M",
            "Qwen3-TTS-1.7B",
        ]));
        let mut rtfs = BTreeMap::new();
        rtfs.insert("Kokoro-82M".to_string(), 0.5);
        rtfs.insert("Qwen3-TTS-1.7B".to_string(), 1.8);
        let plan = build_plan(&classified, &rtfs);
        assert_eq!(plan.stt.as_deref(), Some("Qwen3-ASR-1.7B"));
        assert_eq!(plan.llm.as_deref(), Some("Qwen3-8B-Instruct"));
        assert_eq!(plan.conversation_tts.as_deref(), Some("Kokoro-82M"));
        assert_eq!(plan.listen_tts.as_deref(), Some("Qwen3-TTS-1.7B"));
    }

    #[test]
    fn find_matching_profile_reuses_on_url_and_model() {
        let profiles = vec![
            json!({ "id": "p1", "base_url": "http://localhost:8000", "model": "Qwen3-ASR-1.7B" }),
            json!({ "id": "p2", "base_url": "http://localhost:8000/", "model": "Kokoro-82M" }),
        ];
        // Trailing-slash-insensitive match.
        assert_eq!(
            find_matching_profile(&profiles, "http://localhost:8000/", "Qwen3-ASR-1.7B").as_deref(),
            Some("p1")
        );
        assert_eq!(
            find_matching_profile(&profiles, "http://localhost:8000", "Kokoro-82M").as_deref(),
            Some("p2")
        );
        assert_eq!(find_matching_profile(&profiles, "http://localhost:8000", "nope"), None);
        assert_eq!(find_matching_profile(&profiles, "http://localhost:8000", ""), None);
    }

    fn cfg_with_profiles() -> AppConfig {
        let mut c = AppConfig::default();
        c.model_profiles = vec![
            json!({ "id": "stt1", "name": "ASR", "provider": "omlx", "model": "Qwen3-ASR-1.7B", "base_url": "http://localhost:8000", "capabilities": ["stt"] }),
            json!({ "id": "llm1", "name": "Chat", "provider": "omlx", "model": "Qwen3-8B-Instruct", "base_url": "http://localhost:8000", "capabilities": ["llm"] }),
            json!({ "id": "unrelated", "name": "Other", "provider": "openai", "model": "gpt-4o", "base_url": "https://api.openai.com", "capabilities": ["llm"] }),
        ];
        c.stt_profile = "stt1".into();
        c.llm_profile = "llm1".into();
        c.tts_profile = String::new();
        c.dictation_mode = "polish".into();
        c.translate_target = "German".into();
        // Still-live speech field: listen voice + sts_llm_profile (the
        // call.default composite's fallback rung). sts_voice_profile/
        // sts_voice are set too (still real AppConfig fields, still captured
        // by ScenarioAssignments/the models section below) but are no longer
        // SpeechSection-snapshotted as of Task 14 — see SpeechSection's doc
        // comment.
        c.listen_voice_profile = "listen-profile".into();
        c.listen_voice = "listen-voice".into();
        c.sts_llm_profile = "convo-llm-profile".into();
        c.sts_voice_profile = "convo-voice-profile".into();
        c.sts_voice = "convo-voice".into();
        // Workflow-engine overlays (Task 11: these are what the dictation
        // section now snapshots instead of modes.json).
        c.workflows = vec![WorkflowDef {
            id: "wf.custom-1".into(),
            name: "My Recipe".into(),
            icon: "✨".into(),
            hotkey: String::new(),
            triggers: Vec::new(),
            source: "src.selection".into(),
            processors: Vec::new(),
            outputs: vec!["out.clipboard".into()],
            builtin: false,
        }];
        c.widgets = vec![WidgetDef {
            id: "llm.custom-1".into(),
            role: crate::workflow::model::WidgetRole::Processor,
            type_tag: "llm".into(),
            name: "My Widget".into(),
            icon: String::new(),
            props: json!({ "system": "Be terse." }),
            builtin: false,
        }];
        // Custom vocabulary.
        c.vocab_books = vec![crate::vocab::VocabBook {
            id: "vb1".into(),
            name: "Med Terms".into(),
            enabled: true,
            terms: vec!["myocardium".into()],
            rules: Vec::new(),
        }];
        c.global_vocab_books = vec!["vb1".into()];
        // A couple of non-default hotkey bindings.
        c.hotkey_dictation = "option+space".into();
        c.hotkey_listen = "option+l".into();
        c.notebook_hotkey_1 = 42;
        c
    }

    #[test]
    fn snapshot_captures_referenced_profiles_and_assignments() {
        let c = cfg_with_profiles();
        let snap = snapshot_current(&c, "My Local", true, false, false, false, false);
        assert_eq!(snap.name, "My Local");
        assert!(snap.id.starts_with("saved-my-local-"));
        let m = snap.models.as_ref().expect("models section present");
        // Only stt1 + llm1 are referenced; "unrelated" is not captured.
        let captured: Vec<&str> = m.profiles.iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(captured, vec!["stt1", "llm1"]);
        assert_eq!(m.assignments.stt_profile, "stt1");
        assert_eq!(m.assignments.llm_profile, "llm1");
    }

    #[test]
    fn snapshot_is_section_selective() {
        let c = cfg_with_profiles();
        // Models only.
        let s = snapshot_current(&c, "M", true, false, false, false, false);
        assert!(s.models.is_some() && s.dictation.is_none() && s.speech.is_none());
        assert_eq!(s.sections(), vec!["models"]);
        // Dictation only — carries the workflow/widget overlays + config
        // fields; `user_modes` is always null on a new snapshot.
        let s = snapshot_current(&c, "D", false, true, false, false, false);
        assert_eq!(s.sections(), vec!["dictation"]);
        let d = s.dictation.unwrap();
        assert_eq!(d.dictation_mode, "polish");
        assert_eq!(d.translate_target, "German");
        assert!(d.user_modes.is_null());
        assert_eq!(d.user_workflows.len(), 1);
        assert_eq!(d.user_workflows[0].id, "wf.custom-1");
        assert_eq!(d.user_widgets.len(), 1);
        assert_eq!(d.user_widgets[0].id, "llm.custom-1");
        // Speech only — the still-live fields.
        let s = snapshot_current(&c, "S", false, false, true, false, false);
        assert_eq!(s.sections(), vec!["speech"]);
        let sp = s.speech.unwrap();
        assert_eq!(sp.listen_voice_profile, "listen-profile");
        assert_eq!(sp.listen_voice, "listen-voice");
        assert_eq!(sp.sts_llm_profile, "convo-llm-profile");
        // All five.
        let s = snapshot_current(&c, "All", true, true, true, true, true);
        assert_eq!(s.sections(), vec!["models", "dictation", "speech", "vocab", "hotkeys"]);
    }

    #[test]
    fn snapshot_vocab_and_hotkeys_selective() {
        let c = cfg_with_profiles();
        // Vocab only — captures books + global ids, nothing else.
        let s = snapshot_current(&c, "V", false, false, false, true, false);
        assert_eq!(s.sections(), vec!["vocab"]);
        let v = s.vocab.unwrap();
        assert_eq!(v.vocab_books.len(), 1);
        assert_eq!(v.vocab_books[0].id, "vb1");
        assert_eq!(v.global_vocab_books, vec!["vb1".to_string()]);
        // Hotkeys only — captures every binding.
        let s = snapshot_current(&c, "H", false, false, false, false, true);
        assert_eq!(s.sections(), vec!["hotkeys"]);
        let h = s.hotkeys.unwrap();
        assert_eq!(h.hotkey_dictation, "option+space");
        assert_eq!(h.hotkey_listen, "option+l");
        assert_eq!(h.notebook_hotkey_1, 42);
    }

    #[test]
    fn sections_reports_present_badges() {
        let mut s = SavedScenario::default();
        assert!(s.sections().is_empty());
        s.models = Some(ModelsSection::default());
        s.speech = Some(SpeechSection::default());
        s.hotkeys = Some(HotkeysSection::default());
        // Fixed order regardless of assignment order.
        assert_eq!(s.sections(), vec!["models", "speech", "hotkeys"]);
        s.dictation = Some(DictationSection::default());
        s.vocab = Some(VocabSection::default());
        assert_eq!(s.sections(), vec!["models", "dictation", "speech", "vocab", "hotkeys"]);
    }

    #[test]
    fn snapshot_then_apply_all_sections_round_trips() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Full", true, true, true, true, true);

        let mut target = AppConfig::default();
        apply_saved(&mut target, &snap);
        // Models restored.
        assert_eq!(target.stt_profile, "stt1");
        assert_eq!(target.llm_profile, "llm1");
        assert!(target.model_profiles.iter().any(|p| p["id"].as_str() == Some("stt1")));
        // Speech restored (the still-live fields).
        assert_eq!(target.listen_voice_profile, "listen-profile");
        assert_eq!(target.listen_voice, "listen-voice");
        assert_eq!(target.sts_llm_profile, "convo-llm-profile");
        // sts_voice_profile/sts_voice are restored here too, but via the
        // *models* section's ScenarioAssignments (included in this "all
        // sections" snapshot) — Task 14 dropped them from SpeechSection.
        assert_eq!(target.sts_voice_profile, "convo-voice-profile");
        assert_eq!(target.sts_voice, "convo-voice");
        // Vocab restored.
        assert_eq!(target.vocab_books.len(), 1);
        assert_eq!(target.vocab_books[0].id, "vb1");
        assert_eq!(target.global_vocab_books, vec!["vb1".to_string()]);
        // Hotkeys restored.
        assert_eq!(target.hotkey_dictation, "option+space");
        assert_eq!(target.hotkey_listen, "option+l");
        assert_eq!(target.notebook_hotkey_1, 42);
        // Dictation config + workflow/widget overlays restored.
        assert_eq!(target.dictation_mode, "polish");
        assert_eq!(target.translate_target, "German");
        assert!(target.workflows.iter().any(|w| w.id == "wf.custom-1"));
        assert!(target.widgets.iter().any(|w| w.id == "llm.custom-1"));
    }

    #[test]
    fn apply_vocab_and_hotkeys_are_selective() {
        let source = cfg_with_profiles();

        // Vocab-only apply overwrites vocab, leaves models + hotkeys untouched.
        let vocab = snapshot_current(&source, "Vocab", false, false, false, true, false);
        let mut target = AppConfig::default();
        target.stt_profile = "keep-stt".into();
        let default_hotkey = target.hotkey_dictation.clone();
        apply_saved(&mut target, &vocab);
        assert_eq!(target.stt_profile, "keep-stt", "vocab apply leaves models untouched");
        assert_eq!(target.hotkey_dictation, default_hotkey, "vocab apply leaves hotkeys untouched");
        assert_eq!(target.vocab_books.len(), 1);
        assert_eq!(target.global_vocab_books, vec!["vb1".to_string()]);

        // Hotkeys-only apply overwrites bindings, leaves vocab untouched.
        let hotkeys = snapshot_current(&source, "Keys", false, false, false, false, true);
        let mut target = AppConfig::default();
        target.global_vocab_books = vec!["keep-book".into()];
        apply_saved(&mut target, &hotkeys);
        assert_eq!(target.hotkey_dictation, "option+space");
        assert_eq!(target.notebook_hotkey_1, 42);
        assert_eq!(target.global_vocab_books, vec!["keep-book".to_string()], "hotkeys apply leaves vocab untouched");
    }

    #[test]
    fn apply_saved_upserts_without_deleting_unrelated() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Local", true, false, false, false, false);

        // A different machine: only has an unrelated profile + a stale stt1.
        let mut target = AppConfig::default();
        target.model_profiles = vec![
            json!({ "id": "keepme", "name": "Keep", "provider": "openai", "model": "gpt-4o", "base_url": "https://api.openai.com", "capabilities": ["llm"] }),
            json!({ "id": "stt1", "name": "STALE", "provider": "omlx", "model": "old", "base_url": "http://localhost:9999", "capabilities": ["stt"] }),
        ];

        apply_saved(&mut target, &snap);

        // stt1 upserted (replaced), llm1 appended, keepme untouched.
        let by_id = |id: &str| target.model_profiles.iter().find(|p| p["id"].as_str() == Some(id)).cloned();
        assert!(by_id("keepme").is_some(), "unrelated profile must survive");
        assert_eq!(by_id("stt1").unwrap()["model"].as_str(), Some("Qwen3-ASR-1.7B"));
        assert!(by_id("llm1").is_some(), "referenced profile appended");
        assert_eq!(target.stt_profile, "stt1");
        assert_eq!(target.llm_profile, "llm1");
    }

    #[test]
    fn apply_only_touches_present_sections() {
        let source = cfg_with_profiles();

        // Speech-only scenario must not clear the target's model assignments.
        let speech = snapshot_current(&source, "Speech", false, false, true, false, false);
        let mut target = AppConfig::default();
        target.stt_profile = "keep-stt".into();
        target.llm_profile = "keep-llm".into();
        apply_saved(&mut target, &speech);
        assert_eq!(target.stt_profile, "keep-stt", "speech apply leaves models untouched");
        assert_eq!(target.llm_profile, "keep-llm");
        assert_eq!(target.sts_llm_profile, "convo-llm-profile");
        // sts_voice_profile is a *models*-section (ScenarioAssignments)
        // field now (Task 14) — a speech-only apply must leave it alone.
        assert_eq!(target.sts_voice_profile, "", "speech apply no longer touches sts_voice_profile");

        // Dictation apply sets config fields and upserts the workflow/widget
        // overlays, leaving unrelated existing overlays alone.
        let dict = snapshot_current(&source, "Dict", false, true, false, false, false);
        let mut target = AppConfig::default();
        target.workflows = vec![WorkflowDef {
            id: "wf.other".into(),
            name: "Other".into(),
            icon: String::new(),
            hotkey: String::new(),
            triggers: Vec::new(),
            source: "src.selection".into(),
            processors: Vec::new(),
            outputs: vec!["out.clipboard".into()],
            builtin: false,
        }];
        apply_saved(&mut target, &dict);
        assert_eq!(target.dictation_mode, "polish");
        assert_eq!(target.translate_target, "German");
        assert!(target.workflows.iter().any(|w| w.id == "wf.other"), "unrelated workflow survives");
        assert!(target.workflows.iter().any(|w| w.id == "wf.custom-1"), "snapshot's workflow lands");
    }

    #[test]
    fn snapshot_v2_dictation_roundtrip_never_writes_user_modes() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "V2", false, true, false, false, false);
        let d = snap.dictation.as_ref().unwrap();
        assert!(d.user_modes.is_null(), "new snapshots never populate the legacy user_modes field");

        // Round-trips through JSON exactly as saved-scenario storage does.
        let text = serde_json::to_string(&snap).unwrap();
        let back: SavedScenario = serde_json::from_str(&text).unwrap();
        let d2 = back.dictation.unwrap();
        assert_eq!(d2.user_workflows, d.user_workflows);
        assert_eq!(d2.user_widgets, d.user_widgets);
    }

    #[test]
    fn dictation_section_without_v2_keys_deserializes_to_empty_overlays() {
        // A pre-Task-11 scenario's dictation section carries no
        // user_workflows/user_widgets keys at all.
        let json = r#"{"user_modes": null, "dictation_mode": "raw", "translate_target": "English"}"#;
        let d: DictationSection = serde_json::from_str(json).unwrap();
        assert!(d.user_workflows.is_empty());
        assert!(d.user_widgets.is_empty());
    }

    #[test]
    fn old_snapshot_user_modes_converts_to_llm_widget_via_migration() {
        // A pre-Task-11 scenario: modes.json-shaped user_modes, no
        // user_workflows/user_widgets.
        let legacy_modes = json!({
            "my-custom-mode": {
                "name": "My Custom Mode",
                "icon": "🔧",
                "system": "You write concise bug reports.",
                "user_template": "{text}",
                "temperature": 0.2,
            },
            // A content-less custom mode (no system/user_template) must NOT
            // produce a widget — mirrors migrate_to_workflows rule 1.
            "blank-mode": { "name": "Blank" },
        });
        let scenario = SavedScenario {
            dictation: Some(DictationSection {
                user_modes: legacy_modes,
                user_workflows: Vec::new(),
                user_widgets: Vec::new(),
                dictation_mode: "polish".into(),
                translate_target: "English".into(),
            }),
            ..Default::default()
        };

        let mut target = AppConfig::default();
        apply_saved(&mut target, &scenario);

        assert_eq!(target.dictation_mode, "polish");
        let widget = target
            .widgets
            .iter()
            .find(|w| w.id == "llm.my-custom-mode")
            .expect("content-bearing custom mode becomes an llm.* widget");
        assert_eq!(widget.props["system"], "You write concise bug reports.");
        assert!(
            !target.widgets.iter().any(|w| w.id == "llm.blank-mode"),
            "content-less mode produces no widget"
        );
        // The live config's own migration state is untouched — conversion ran
        // against a throwaway scratch config, not the real one.
        assert!(!target.workflow_migration_done);
    }

    #[test]
    fn old_snapshot_apply_never_resurrects_dead_hotkey_fields() {
        // A pre-Task-11 scenario's hotkeys section, serialized when
        // hotkey_agent/hotkey_agent_panel/hotkey_meeting/hotkey_sts were still
        // part of the schema.
        let json = r#"{
            "hotkey_dictation": "cmd+shift+d",
            "hotkey_agent": "cmd+shift+a",
            "hotkey_agent_panel": "cmd+shift+g",
            "hotkey_meeting": "option+m",
            "hotkey_sts": "option+s"
        }"#;
        let h: HotkeysSection = serde_json::from_str(json).unwrap();
        // The dead fields simply aren't part of the struct anymore.
        let scenario = SavedScenario { hotkeys: Some(h), ..Default::default() };

        let mut target = AppConfig::default();
        target.hotkey_agent = "".into();
        target.hotkey_agent_panel = "".into();
        target.hotkey_meeting = "".into();
        target.hotkey_sts = "".into();
        apply_saved(&mut target, &scenario);

        assert_eq!(target.hotkey_dictation, "cmd+shift+d", "the surviving field still applies");
        assert_eq!(target.hotkey_agent, "", "hotkey_agent cannot be resurrected — dropped from the schema");
        assert_eq!(target.hotkey_agent_panel, "");
        assert_eq!(target.hotkey_meeting, "");
        assert_eq!(target.hotkey_sts, "");
    }

    #[test]
    fn parse_saved_scenario_validates_shape_and_refreshes_id() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Shareable", true, false, false, false, false);
        let mut value = serde_json::to_value(&snap).unwrap();
        value["id"] = json!("original-id-should-be-replaced");
        let text = serde_json::to_string(&value).unwrap();

        let parsed = parse_saved_scenario(&text).unwrap();
        assert_ne!(parsed.id, "original-id-should-be-replaced");
        assert!(parsed.id.starts_with("saved-shareable-"));
        assert_eq!(parsed.models.unwrap().profiles.len(), 2);

        // Garbage / wrong-shape input is rejected.
        assert!(parse_saved_scenario("{\"hello\":true}").is_err());
        assert!(parse_saved_scenario("not json").is_err());
    }

    #[test]
    fn parse_tolerates_legacy_flat_shape() {
        // An old export with top-level profiles + assignments migrates into a
        // models section rather than being rejected.
        let legacy = json!({
            "id": "saved-old",
            "name": "Legacy",
            "created_at": "1700000000",
            "profiles": [
                { "id": "stt1", "name": "ASR", "provider": "omlx", "model": "Qwen3-ASR-1.7B", "base_url": "http://localhost:8000", "capabilities": ["stt"] }
            ],
            "assignments": { "stt_profile": "stt1", "llm_profile": "" }
        });
        let parsed = parse_saved_scenario(&legacy.to_string()).unwrap();
        assert_eq!(parsed.sections(), vec!["models"]);
        let m = parsed.models.unwrap();
        assert_eq!(m.profiles.len(), 1);
        assert_eq!(m.assignments.stt_profile, "stt1");
    }

    // ── text-action bindings (dead-key regression) ──────────────────────────

    #[test]
    fn old_scenario_apply_preserves_current_text_actions_and_converts_legacy() {
        let mut cfg = AppConfig::default();
        cfg.text_actions = vec![crate::config::TextActionBinding {
            hotkey: "cmd+shift+y".into(),
            mode_id: "translate".into(),
            output_target: crate::config::OutputTarget::FloatingPopup,
        }];

        // An old saved scenario's hotkeys section, serialized before text_actions
        // existed: no "text_actions" key at all, but a live legacy combo.
        let json = r#"{"hotkey_transform": "cmd+shift+t"}"#;
        let h: HotkeysSection = serde_json::from_str(json).unwrap();
        assert!(h.text_actions.is_none(), "missing key must deserialize to None, not Some([])");

        let scenario = SavedScenario { hotkeys: Some(h), ..Default::default() };
        apply_saved(&mut cfg, &scenario);

        // The user's current bindings survive untouched...
        assert_eq!(cfg.text_actions.len(), 1);
        assert_eq!(cfg.text_actions[0].hotkey, "cmd+shift+y");
        assert_eq!(cfg.text_actions[0].mode_id, "translate");
        // ...and the legacy combo is cleared rather than resurrected as a dead key.
        assert!(cfg.hotkey_transform.is_empty());
    }

    #[test]
    fn old_scenario_apply_onto_empty_bindings_migrates_legacy() {
        let mut cfg = AppConfig::default();
        cfg.text_actions = Vec::new();
        cfg.transform_mode = String::new(); // force the "polish" fallback path

        let json = r#"{"hotkey_transform": "cmd+shift+t"}"#;
        let h: HotkeysSection = serde_json::from_str(json).unwrap();
        let scenario = SavedScenario { hotkeys: Some(h), ..Default::default() };

        apply_saved(&mut cfg, &scenario);

        assert_eq!(cfg.text_actions.len(), 1);
        let b = &cfg.text_actions[0];
        assert_eq!(b.hotkey, "cmd+shift+t");
        assert_eq!(b.mode_id, "polish");
        assert_eq!(b.output_target, crate::config::OutputTarget::ActiveTextField);
        assert!(cfg.hotkey_transform.is_empty());
    }

    #[test]
    fn snapshot_roundtrip_restores_text_actions() {
        let mut source = cfg_with_profiles();
        source.text_actions = vec![
            crate::config::TextActionBinding {
                hotkey: "cmd+shift+y".into(),
                mode_id: "translate".into(),
                output_target: crate::config::OutputTarget::FloatingPopup,
            },
            crate::config::TextActionBinding {
                hotkey: "cmd+shift+u".into(),
                mode_id: "polish".into(),
                output_target: crate::config::OutputTarget::ActiveTextField,
            },
        ];

        let snap = snapshot_current(&source, "TA", false, false, false, false, true);
        // Round-trip through JSON, exactly as saved-scenario storage does.
        let text = serde_json::to_string(&snap).unwrap();
        let deserialized: SavedScenario = serde_json::from_str(&text).unwrap();

        let mut target = AppConfig::default();
        target.text_actions = vec![crate::config::TextActionBinding {
            hotkey: "existing".into(),
            mode_id: "old".into(),
            output_target: crate::config::OutputTarget::Clipboard,
        }];
        target.hotkey_transform = "cmd+shift+t".into();

        apply_saved(&mut target, &deserialized);

        assert_eq!(target.text_actions, source.text_actions);
        assert!(target.hotkey_transform.is_empty());
    }
}
