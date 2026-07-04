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

/// A self-contained, shareable snapshot of a working configuration: the model
/// profiles referenced by the default assignments, plus those assignments.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedScenario {
    /// Stable unique id.
    pub id: String,
    /// Human-friendly name.
    pub name: String,
    /// Creation time as a unix-epoch-seconds string (formatted by the frontend).
    pub created_at: String,
    /// Snapshot of the model-profile JSON entries the assignments reference.
    pub profiles: Vec<serde_json::Value>,
    /// The default-service assignments to restore.
    pub assignments: ScenarioAssignments,
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

/// Snapshot the live config into a [`SavedScenario`] named `name`.
///
/// Captures every model profile referenced by the current default assignments
/// (STT / LLM / TTS / conversation / listen), de-duplicated, plus the
/// assignment values themselves.
pub fn snapshot_current(config: &AppConfig, name: &str) -> SavedScenario {
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
    SavedScenario {
        id: generate_scenario_id(name),
        name: name.to_string(),
        created_at: epoch_secs_string(),
        profiles,
        assignments: ScenarioAssignments {
            stt_profile: config.stt_profile.clone(),
            llm_profile: config.llm_profile.clone(),
            tts_profile: config.tts_profile.clone(),
            sts_voice_profile: config.sts_voice_profile.clone(),
            listen_voice_profile: config.listen_voice_profile.clone(),
            sts_voice: config.sts_voice.clone(),
            listen_voice: config.listen_voice.clone(),
        },
    }
}

/// Apply a [`SavedScenario`] to `config`: upsert its snapshot profiles by id
/// (replacing a same-id profile, else appending — **never** deleting unrelated
/// profiles), then restore the default assignments.
pub fn apply_saved(config: &mut AppConfig, scenario: &SavedScenario) {
    for p in &scenario.profiles {
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
    let a = &scenario.assignments;
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

/// Parse and validate an imported scenario JSON string, returning it with a
/// **fresh id** (so imports never collide with existing saved scenarios).
///
/// Rejects any JSON that isn't an object carrying an `assignments` object and a
/// `profiles` array, so an arbitrary file produces a clear error rather than a
/// silently-defaulted empty scenario.
pub fn parse_saved_scenario(json: &str) -> Result<SavedScenario, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    let shape_ok = value.is_object()
        && value.get("assignments").map_or(false, |a| a.is_object())
        && value.get("profiles").map_or(false, |p| p.is_array());
    if !shape_ok {
        return Err("not a fonos scenario file".to_string());
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
        c
    }

    #[test]
    fn snapshot_captures_referenced_profiles_and_assignments() {
        let c = cfg_with_profiles();
        let snap = snapshot_current(&c, "My Local");
        assert_eq!(snap.name, "My Local");
        assert!(snap.id.starts_with("saved-my-local-"));
        // Only stt1 + llm1 are referenced; "unrelated" is not captured.
        let captured: Vec<&str> = snap.profiles.iter().map(|p| p["id"].as_str().unwrap()).collect();
        assert_eq!(captured, vec!["stt1", "llm1"]);
        assert_eq!(snap.assignments.stt_profile, "stt1");
        assert_eq!(snap.assignments.llm_profile, "llm1");
    }

    #[test]
    fn apply_saved_upserts_without_deleting_unrelated() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Local");

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
    fn parse_saved_scenario_validates_shape_and_refreshes_id() {
        let source = cfg_with_profiles();
        let snap = snapshot_current(&source, "Shareable");
        let mut value = serde_json::to_value(&snap).unwrap();
        value["id"] = json!("original-id-should-be-replaced");
        let text = serde_json::to_string(&value).unwrap();

        let parsed = parse_saved_scenario(&text).unwrap();
        assert_ne!(parsed.id, "original-id-should-be-replaced");
        assert!(parsed.id.starts_with("saved-shareable-"));
        assert_eq!(parsed.profiles.len(), 2);

        // Garbage / wrong-shape input is rejected.
        assert!(parse_saved_scenario("{\"hello\":true}").is_err());
        assert!(parse_saved_scenario("not json").is_err());
    }
}
