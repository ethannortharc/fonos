//! Model-profile resolution: `AppConfig.model_profiles` JSON → [`ServiceConfig`].
//!
//! Pure functions over an already-loaded [`AppConfig`]; platform shells wrap
//! these with their own state/locking (e.g. the Tauri `AppState` mutex).

use crate::config::AppConfig;
use crate::llm::ServiceConfig;
use crate::workflow::engine::effective_widgets;

/// Default base URL per provider when the profile leaves it empty.
fn default_base_url(provider: &str) -> String {
    match provider {
        "omlx" => "http://localhost:8000".to_string(),
        "ollama" => "http://localhost:11434".to_string(),
        "openai" => "https://api.openai.com".to_string(),
        "openrouter" => "https://openrouter.ai/api/v1".to_string(),
        "anthropic" => "https://api.anthropic.com".to_string(),
        "google" => "https://generativelanguage.googleapis.com".to_string(),
        _ => String::new(),
    }
}

/// Build a [`ServiceConfig`] from one JSON model-profile entry.
pub fn service_from_profile(profile: &serde_json::Value) -> ServiceConfig {
    let url = profile["base_url"].as_str().unwrap_or("").to_string();
    ServiceConfig {
        base_url: if url.is_empty() {
            default_base_url(profile["provider"].as_str().unwrap_or(""))
        } else {
            url.trim_end_matches('/').to_string()
        },
        api_key: profile["api_key"].as_str().unwrap_or("").to_string(),
        model: profile["model"].as_str().unwrap_or("").to_string(),
        provider: profile["provider"].as_str().unwrap_or("").to_string(),
        stt_api: profile["stt_api"].as_str().unwrap_or("whisper").to_string(),
    }
}

fn empty_service() -> ServiceConfig {
    ServiceConfig {
        base_url: String::new(),
        api_key: String::new(),
        model: String::new(),
        provider: String::new(),
        stt_api: "whisper".to_string(),
    }
}

/// Resolve the active profile for a service role (`"stt"`, `"tts"`, anything
/// else = LLM) into connection info.
pub fn resolve_service(config: &AppConfig, service: &str) -> ServiceConfig {
    let profile_id = match service {
        "stt" => &config.stt_profile,
        "tts" => &config.tts_profile,
        _ => &config.llm_profile,
    };
    if profile_id.is_empty() {
        return empty_service();
    }
    resolve_profile(config, profile_id)
}

/// Resolve a specific profile by id into connection info.
pub fn resolve_profile(config: &AppConfig, profile_id: &str) -> ServiceConfig {
    config
        .model_profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .map(service_from_profile)
        .unwrap_or_else(empty_service)
}

/// The `stt.default` widget's `model_profile` prop from the effective widget set,
/// or `""` when the widget is absent (never — built-ins can't be deleted) or the
/// prop is unset/non-string.
fn stt_default_model_profile(config: &AppConfig) -> String {
    effective_widgets(config)
        .iter()
        .find(|w| w.id == "stt.default")
        .and_then(|w| w.props.get("model_profile"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// True when `model_profiles` contains an entry whose `id` equals `id`.
fn profile_exists(config: &AppConfig, id: &str) -> bool {
    config
        .model_profiles
        .iter()
        .any(|p| p["id"].as_str() == Some(id))
}

/// The profile id dictation will actually feed to STT for a plain (no per-call
/// override) run: the `stt.default` widget's non-empty `model_profile` if set,
/// else the non-empty global [`AppConfig::stt_profile`], else `None`. The
/// returned id may be the `"apple-speech"` sentinel (an on-device engine, not a
/// `model_profiles` entry) rather than a real profile id — callers that need a
/// live/usable check should use [`is_stt_effectively_configured`].
///
/// Mirrors the resolution order in `commands/dictation.rs` (the `stt_profile_override
/// == None` arms): widget `model_profile` first, global default last. It never
/// falls back from a set-but-unusable widget ref to the global — see
/// [`is_stt_effectively_configured`] for why that matters.
pub fn effective_stt_profile(config: &AppConfig) -> Option<String> {
    let widget = stt_default_model_profile(config);
    if !widget.is_empty() {
        return Some(widget);
    }
    if !config.stt_profile.is_empty() {
        return Some(config.stt_profile.clone());
    }
    None
}

/// Whether dictation will actually transcribe with the current config — the
/// single runtime-backed STT gate. Mirrors `commands/dictation.rs`'s STT
/// resolution (the plain-run, no-override arms) **exactly**:
///
/// - `stt.default` widget's `model_profile` set → the `"apple-speech"` sentinel
///   is usable on macOS only (on-device engine; see the platform gate below);
///   any other id is usable iff a `model_profiles` entry with that id still
///   exists.
/// - `model_profile` empty → the global [`AppConfig::stt_profile`] must be
///   non-empty **and** still reference an existing `model_profiles` entry. The
///   global arm is a plain `resolve_service("stt")` with no sentinel handling,
///   so a literal `"apple-speech"` global id is *not* special-cased here.
///
/// Poisoning: because the runtime picks the widget ref before the global and
/// **never falls back** when it's set, a widget `model_profile` pointing at a
/// since-deleted profile makes STT unconfigured even when the global default is
/// perfectly valid — the dangling widget ref shadows and poisons it. Capability
/// tags are irrelevant: only the assigned default is authoritative.
pub fn is_stt_effectively_configured(config: &AppConfig) -> bool {
    let widget = stt_default_model_profile(config);
    if !widget.is_empty() {
        // The "apple-speech" sentinel only counts on macOS: the helper binary
        // cannot exist elsewhere and the dictation pipeline returns an explicit
        // error there — a macOS config imported onto Linux must NOT pass this
        // gate (it would skip onboarding and then fail every dictation).
        return (widget == "apple-speech" && cfg!(target_os = "macos"))
            || profile_exists(config, &widget);
    }
    !config.stt_profile.is_empty() && profile_exists(config, &config.stt_profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `model_profiles` JSON entry with the given id (no capabilities needed —
    /// the STT gate is assignment-based, not capability-based).
    fn profile(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": id, "provider": "openai", "model": "m" })
    }

    /// A config carrying an `stt.default` widget-def override whose
    /// `model_profile` prop is `mp` (overlaid onto the built-in by
    /// `effective_widgets`).
    fn cfg_with_widget_mp(mp: &str, profiles: Vec<serde_json::Value>, global: &str) -> AppConfig {
        let widget: crate::workflow::model::WidgetDef = serde_json::from_value(serde_json::json!({
            "id": "stt.default",
            "role": "processor",
            "type_tag": "stt",
            "name": "STT",
            "props": { "model_profile": mp },
        }))
        .unwrap();
        AppConfig {
            widgets: vec![widget],
            model_profiles: profiles,
            stt_profile: global.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn widget_set_and_profile_exists_is_configured() {
        let cfg = cfg_with_widget_mp("p1", vec![profile("p1")], "");
        assert!(is_stt_effectively_configured(&cfg));
        assert_eq!(effective_stt_profile(&cfg).as_deref(), Some("p1"));
    }

    #[test]
    fn widget_apple_speech_sentinel_is_configured() {
        // The sentinel is not a model_profiles entry, and it's only usable on
        // macOS (see is_stt_effectively_configured's platform gate) — a plain
        // `assert!` here would fail when this suite runs on Linux CI.
        let cfg = cfg_with_widget_mp("apple-speech", vec![], "");
        assert_eq!(is_stt_effectively_configured(&cfg), cfg!(target_os = "macos"));
        assert_eq!(effective_stt_profile(&cfg).as_deref(), Some("apple-speech"));
    }

    #[test]
    fn apple_speech_sentinel_only_counts_on_macos() {
        // The helper binary behind the "apple-speech" sentinel can't exist off
        // macOS, so the gate must reject it there even though the widget is
        // "assigned" — otherwise a macOS config imported onto Linux would skip
        // onboarding and then fail every dictation attempt.
        let cfg = cfg_with_widget_mp("apple-speech", vec![], "");
        assert_eq!(is_stt_effectively_configured(&cfg), cfg!(target_os = "macos"));
    }

    #[test]
    fn dangling_widget_ref_poisons_valid_global() {
        // Widget points at a since-deleted profile; the global default is valid.
        // Runtime picks the widget ref and never falls back → unconfigured.
        let cfg = cfg_with_widget_mp("ghost", vec![profile("real")], "real");
        assert!(!is_stt_effectively_configured(&cfg));
        assert_eq!(effective_stt_profile(&cfg).as_deref(), Some("ghost"));
    }

    #[test]
    fn global_only_valid_is_configured() {
        // Widget prop empty → fall through to the global default, which exists.
        let mut cfg = AppConfig {
            model_profiles: vec![profile("g1")],
            stt_profile: "g1".to_string(),
            ..Default::default()
        };
        assert!(is_stt_effectively_configured(&cfg));
        // Even without a capabilities array (older profiles predate it).
        cfg.model_profiles = vec![serde_json::json!({ "id": "g1", "name": "x" })];
        assert!(is_stt_effectively_configured(&cfg));
        assert_eq!(effective_stt_profile(&cfg).as_deref(), Some("g1"));
    }

    #[test]
    fn global_dangling_is_not_configured() {
        let cfg = AppConfig {
            model_profiles: vec![profile("real")],
            stt_profile: "ghost".to_string(),
            ..Default::default()
        };
        assert!(!is_stt_effectively_configured(&cfg));
    }

    #[test]
    fn nothing_configured_is_false() {
        let cfg = AppConfig::default();
        assert!(!is_stt_effectively_configured(&cfg));
        assert_eq!(effective_stt_profile(&cfg), None);
    }

    #[test]
    fn capability_tagged_but_unassigned_is_not_configured() {
        // A profile advertising "stt" is unusable until it's the assigned
        // default — mirrors runtime, which reads the assignment, not caps.
        let cfg = AppConfig {
            model_profiles: vec![serde_json::json!({
                "id": "cap", "name": "x", "provider": "openai",
                "model": "m", "capabilities": ["stt"]
            })],
            stt_profile: String::new(),
            ..Default::default()
        };
        assert!(!is_stt_effectively_configured(&cfg));
    }
}
