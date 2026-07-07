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

/// The **dictation** section of a saved scenario: the user's custom modes (as
/// stored verbatim in `modes.json`) plus the two config fields that drive
/// dictation behaviour. Applied by writing modes.json (shell) + config fields
/// (core).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DictationSection {
    /// The user-modes map exactly as persisted in `modes.json` (id → Mode).
    pub user_modes: serde_json::Value,
    /// Default dictation mode id.
    pub dictation_mode: String,
    /// Target language for translation mode.
    pub translate_target: String,
}

/// The **speech** section of a saved scenario: the Listen queue + STS
/// conversation configuration (voice output pipelines).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechSection {
    /// Mode id used to process captured Listen text.
    pub listen_mode: String,
    /// TTS profile id for Listen synthesis.
    pub listen_voice_profile: String,
    /// Voice identifier for Listen synthesis.
    pub listen_voice: String,
    /// Persona / system prompt for the conversation stage.
    pub sts_persona: String,
    /// LLM profile id for conversation.
    pub sts_llm_profile: String,
    /// TTS profile id for the spoken reply.
    pub sts_voice_profile: String,
    /// Voice identifier for the spoken reply.
    pub sts_voice: String,
    /// Conversation memory: max user/assistant turn pairs kept.
    pub sts_max_turns: usize,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeysSection {
    /// Hold-to-talk dictation combo.
    pub hotkey_dictation: String,
    /// Toggle dictation combo (press to start / stop).
    pub hotkey_dictation_toggle: String,
    /// TTS playback combo.
    pub hotkey_tts: String,
    /// Press-and-hold agent voice combo.
    pub hotkey_agent: String,
    /// Toggle agent panel combo.
    pub hotkey_agent_panel: String,
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
    /// Toggle meeting-mode combo.
    pub hotkey_meeting: String,
    /// Quick-transform combo.
    pub hotkey_transform: String,
    /// Capture-into-Listen-queue combo.
    pub hotkey_listen: String,
    /// Hold-to-talk conversation combo.
    pub hotkey_sts: String,
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
/// * **dictation** — `user_modes` (the modes.json map, supplied by the shell)
///   plus the dictation-mode / translate-target config fields.
/// * **speech** — the Listen + STS conversation configuration.
/// * **vocab** — the custom vocabulary books + globally-applied book ids.
/// * **hotkeys** — every global + notebook hotkey binding.
pub fn snapshot_current(
    config: &AppConfig,
    name: &str,
    user_modes: serde_json::Value,
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
        user_modes,
        dictation_mode: config.dictation_mode.clone(),
        translate_target: config.translate_target.clone(),
    });
    let speech = include_speech.then(|| SpeechSection {
        listen_mode: config.listen_mode.clone(),
        listen_voice_profile: config.listen_voice_profile.clone(),
        listen_voice: config.listen_voice.clone(),
        sts_persona: config.sts_persona.clone(),
        sts_llm_profile: config.sts_llm_profile.clone(),
        sts_voice_profile: config.sts_voice_profile.clone(),
        sts_voice: config.sts_voice.clone(),
        sts_max_turns: config.sts_max_turns,
    });
    let vocab = include_vocab.then(|| VocabSection {
        vocab_books: config.vocab_books.clone(),
        global_vocab_books: config.global_vocab_books.clone(),
    });
    let hotkeys = include_hotkeys.then(|| HotkeysSection {
        hotkey_dictation: config.hotkey_dictation.clone(),
        hotkey_dictation_toggle: config.hotkey_dictation_toggle.clone(),
        hotkey_tts: config.hotkey_tts.clone(),
        hotkey_agent: config.hotkey_agent.clone(),
        hotkey_agent_panel: config.hotkey_agent_panel.clone(),
        hotkey_note: config.hotkey_note.clone(),
        hotkey_note_1: config.hotkey_note_1.clone(),
        hotkey_note_2: config.hotkey_note_2.clone(),
        hotkey_note_3: config.hotkey_note_3.clone(),
        notebook_hotkey_1: config.notebook_hotkey_1,
        notebook_hotkey_2: config.notebook_hotkey_2,
        notebook_hotkey_3: config.notebook_hotkey_3,
        hotkey_meeting: config.hotkey_meeting.clone(),
        hotkey_transform: config.hotkey_transform.clone(),
        hotkey_listen: config.hotkey_listen.clone(),
        hotkey_sts: config.hotkey_sts.clone(),
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
/// * **dictation** — set the dictation-mode / translate-target config fields
///   here; the custom-modes map is **returned** for the shell to persist into
///   `modes.json` (core cannot touch that file).
///
/// Returns `Some(user_modes)` when a dictation section was applied, else `None`.
pub fn apply_saved(config: &mut AppConfig, scenario: &SavedScenario) -> Option<serde_json::Value> {
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
        config.listen_mode = s.listen_mode.clone();
        config.listen_voice_profile = s.listen_voice_profile.clone();
        config.sts_persona = s.sts_persona.clone();
        config.sts_llm_profile = s.sts_llm_profile.clone();
        config.sts_voice_profile = s.sts_voice_profile.clone();
        config.sts_max_turns = s.sts_max_turns;
        if !s.listen_voice.trim().is_empty() {
            config.listen_voice = s.listen_voice.clone();
        }
        if !s.sts_voice.trim().is_empty() {
            config.sts_voice = s.sts_voice.clone();
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
        config.hotkey_agent = h.hotkey_agent.clone();
        config.hotkey_agent_panel = h.hotkey_agent_panel.clone();
        config.hotkey_note = h.hotkey_note.clone();
        config.hotkey_note_1 = h.hotkey_note_1.clone();
        config.hotkey_note_2 = h.hotkey_note_2.clone();
        config.hotkey_note_3 = h.hotkey_note_3.clone();
        config.notebook_hotkey_1 = h.notebook_hotkey_1;
        config.notebook_hotkey_2 = h.notebook_hotkey_2;
        config.notebook_hotkey_3 = h.notebook_hotkey_3;
        config.hotkey_meeting = h.hotkey_meeting.clone();
        config.hotkey_transform = h.hotkey_transform.clone();
        config.hotkey_listen = h.hotkey_listen.clone();
        config.hotkey_sts = h.hotkey_sts.clone();

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
        return Some(d.user_modes.clone());
    }
    None
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
        c.sts_voice_profile = String::new();
        c.listen_voice_profile = String::new();
        c.dictation_mode = "polish".into();
        c.translate_target = "German".into();
        c.listen_mode = "listen".into();
        c.sts_persona = "Be terse.".into();
        c.sts_max_turns = 5;
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

    /// A user-modes map as it would appear in modes.json.
    fn user_modes() -> serde_json::Value {
        json!({ "my-mode": { "name": "My Mode", "icon": "🔧", "temperature": 0.2 } })
    }

    #[test]
    fn snapshot_captures_referenced_profiles_and_assignments() {
        let c = cfg_with_profiles();
        let snap = snapshot_current(&c, "My Local", json!(null), true, false, false, false, false);
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
        let s = snapshot_current(&c, "M", json!(null), true, false, false, false, false);
        assert!(s.models.is_some() && s.dictation.is_none() && s.speech.is_none());
        assert_eq!(s.sections(), vec!["models"]);
        // Dictation only — carries the user-modes map + config fields.
        let s = snapshot_current(&c, "D", user_modes(), false, true, false, false, false);
        assert_eq!(s.sections(), vec!["dictation"]);
        let d = s.dictation.unwrap();
        assert_eq!(d.dictation_mode, "polish");
        assert_eq!(d.translate_target, "German");
        assert_eq!(d.user_modes["my-mode"]["name"], "My Mode");
        // Speech only.
        let s = snapshot_current(&c, "S", json!(null), false, false, true, false, false);
        assert_eq!(s.sections(), vec!["speech"]);
        let sp = s.speech.unwrap();
        assert_eq!(sp.sts_persona, "Be terse.");
        assert_eq!(sp.sts_max_turns, 5);
        // All five.
        let s = snapshot_current(&c, "All", user_modes(), true, true, true, true, true);
        assert_eq!(s.sections(), vec!["models", "dictation", "speech", "vocab", "hotkeys"]);
    }

    #[test]
    fn snapshot_vocab_and_hotkeys_selective() {
        let c = cfg_with_profiles();
        // Vocab only — captures books + global ids, nothing else.
        let s = snapshot_current(&c, "V", json!(null), false, false, false, true, false);
        assert_eq!(s.sections(), vec!["vocab"]);
        let v = s.vocab.unwrap();
        assert_eq!(v.vocab_books.len(), 1);
        assert_eq!(v.vocab_books[0].id, "vb1");
        assert_eq!(v.global_vocab_books, vec!["vb1".to_string()]);
        // Hotkeys only — captures every binding.
        let s = snapshot_current(&c, "H", json!(null), false, false, false, false, true);
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
        let snap = snapshot_current(&source, "Full", user_modes(), true, true, true, true, true);

        let mut target = AppConfig::default();
        let returned = apply_saved(&mut target, &snap).expect("dictation returns user modes");
        // Models restored.
        assert_eq!(target.stt_profile, "stt1");
        assert_eq!(target.llm_profile, "llm1");
        assert!(target.model_profiles.iter().any(|p| p["id"].as_str() == Some("stt1")));
        // Speech restored.
        assert_eq!(target.sts_persona, "Be terse.");
        assert_eq!(target.sts_max_turns, 5);
        // Vocab restored.
        assert_eq!(target.vocab_books.len(), 1);
        assert_eq!(target.vocab_books[0].id, "vb1");
        assert_eq!(target.global_vocab_books, vec!["vb1".to_string()]);
        // Hotkeys restored.
        assert_eq!(target.hotkey_dictation, "option+space");
        assert_eq!(target.hotkey_listen, "option+l");
        assert_eq!(target.notebook_hotkey_1, 42);
        // Dictation config restored + user modes handed back.
        assert_eq!(target.dictation_mode, "polish");
        assert_eq!(target.translate_target, "German");
        assert_eq!(returned["my-mode"]["name"], "My Mode");
    }

    #[test]
    fn apply_vocab_and_hotkeys_are_selective() {
        let source = cfg_with_profiles();

        // Vocab-only apply overwrites vocab, leaves models + hotkeys untouched.
        let vocab = snapshot_current(&source, "Vocab", json!(null), false, false, false, true, false);
        let mut target = AppConfig::default();
        target.stt_profile = "keep-stt".into();
        let default_hotkey = target.hotkey_dictation.clone();
        let modes = apply_saved(&mut target, &vocab);
        assert!(modes.is_none(), "vocab-only apply returns no user modes");
        assert_eq!(target.stt_profile, "keep-stt", "vocab apply leaves models untouched");
        assert_eq!(target.hotkey_dictation, default_hotkey, "vocab apply leaves hotkeys untouched");
        assert_eq!(target.vocab_books.len(), 1);
        assert_eq!(target.global_vocab_books, vec!["vb1".to_string()]);

        // Hotkeys-only apply overwrites bindings, leaves vocab untouched.
        let hotkeys = snapshot_current(&source, "Keys", json!(null), false, false, false, false, true);
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
        let snap = snapshot_current(&source, "Local", json!(null), true, false, false, false, false);

        // A different machine: only has an unrelated profile + a stale stt1.
        let mut target = AppConfig::default();
        target.model_profiles = vec![
            json!({ "id": "keepme", "name": "Keep", "provider": "openai", "model": "gpt-4o", "base_url": "https://api.openai.com", "capabilities": ["llm"] }),
            json!({ "id": "stt1", "name": "STALE", "provider": "omlx", "model": "old", "base_url": "http://localhost:9999", "capabilities": ["stt"] }),
        ];

        let modes = apply_saved(&mut target, &snap);
        assert!(modes.is_none(), "models-only apply returns no user modes");

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
        let speech = snapshot_current(&source, "Speech", json!(null), false, false, true, false, false);
        let mut target = AppConfig::default();
        target.stt_profile = "keep-stt".into();
        target.llm_profile = "keep-llm".into();
        apply_saved(&mut target, &speech);
        assert_eq!(target.stt_profile, "keep-stt", "speech apply leaves models untouched");
        assert_eq!(target.llm_profile, "keep-llm");
        assert_eq!(target.sts_persona, "Be terse.");
        assert_eq!(target.sts_max_turns, 5);

        // Dictation apply sets config fields and returns the user-modes map.
        let dict = snapshot_current(&source, "Dict", user_modes(), false, true, false, false, false);
        let mut target = AppConfig::default();
        let returned = apply_saved(&mut target, &dict).expect("dictation returns user modes");
        assert_eq!(target.dictation_mode, "polish");
        assert_eq!(target.translate_target, "German");
        assert_eq!(returned["my-mode"]["name"], "My Mode");
    }

    #[test]
    fn parse_saved_scenario_validates_shape_and_refreshes_id() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Shareable", json!(null), true, false, false, false, false);
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
            output_target: crate::modes::OutputTarget::FloatingPopup,
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
        assert_eq!(b.output_target, crate::modes::OutputTarget::ActiveTextField);
        assert!(cfg.hotkey_transform.is_empty());
    }

    #[test]
    fn snapshot_roundtrip_restores_text_actions() {
        let mut source = cfg_with_profiles();
        source.text_actions = vec![
            crate::config::TextActionBinding {
                hotkey: "cmd+shift+y".into(),
                mode_id: "translate".into(),
                output_target: crate::modes::OutputTarget::FloatingPopup,
            },
            crate::config::TextActionBinding {
                hotkey: "cmd+shift+u".into(),
                mode_id: "polish".into(),
                output_target: crate::modes::OutputTarget::ActiveTextField,
            },
        ];

        let snap = snapshot_current(&source, "TA", json!(null), false, false, false, false, true);
        // Round-trip through JSON, exactly as saved-scenario storage does.
        let text = serde_json::to_string(&snap).unwrap();
        let deserialized: SavedScenario = serde_json::from_str(&text).unwrap();

        let mut target = AppConfig::default();
        target.text_actions = vec![crate::config::TextActionBinding {
            hotkey: "existing".into(),
            mode_id: "old".into(),
            output_target: crate::modes::OutputTarget::Clipboard,
        }];
        target.hotkey_transform = "cmd+shift+t".into();

        apply_saved(&mut target, &deserialized);

        assert_eq!(target.text_actions, source.text_actions);
        assert!(target.hotkey_transform.is_empty());
    }
}
