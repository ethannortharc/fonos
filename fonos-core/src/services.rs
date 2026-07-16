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

/// Whether an STT ref — either the on-device `"apple-speech"` sentinel or a
/// `model_profiles` entry id — will actually work as a live STT source, i.e.
/// will NOT fall into dictation's `provider == "apple"` branch
/// (`commands/dictation.rs:1035`) on a platform where the Apple Speech helper
/// binary can't exist. Mirrors that branch's own `cfg!(target_os = "macos")`
/// gate exactly, so this config-time check can never disagree with the
/// runtime outcome:
///
/// - the literal `"apple-speech"` sentinel (not itself a `model_profiles`
///   entry — see [`effective_stt_profile`]'s doc) is usable only on macOS;
/// - a ref that doesn't resolve to any `model_profiles` entry is never
///   usable (a dangling reference, same as before this fix);
/// - a resolved profile whose `provider` is `"apple"` — e.g. the REAL
///   `scenario-apple-stt` profile `appleSttSeed.ts` seeds on first run, the
///   same on-device engine as the sentinel but addressed by a stable
///   `model_profiles` id instead of the magic string — is usable only on
///   macOS, for the identical reason as the sentinel: `resolve_service` /
///   `resolve_profile` copy `provider` straight from the profile JSON, so
///   this id reaches the exact same `svc.provider == "apple"` branch;
/// - any other resolved profile is usable regardless of platform — its
///   provider talks HTTP, which works everywhere.
fn stt_ref_usable(config: &AppConfig, id: &str) -> bool {
    if id == "apple-speech" {
        return cfg!(target_os = "macos");
    }
    match config.model_profiles.iter().find(|p| p["id"].as_str() == Some(id)) {
        None => false,
        Some(p) if p["provider"].as_str() == Some("apple") => cfg!(target_os = "macos"),
        Some(_) => true,
    }
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
/// - `stt.default` widget's `model_profile` set → usable per [`stt_ref_usable`]:
///   the `"apple-speech"` sentinel or a real `model_profiles` entry whose
///   `provider` is `"apple"` count only on macOS (on-device engine — the
///   helper binary can't exist elsewhere, and dictation's `provider ==
///   "apple"` branch returns an explicit error there); any other existing
///   profile id counts everywhere; a dangling id never counts.
/// - `model_profile` empty → the global [`AppConfig::stt_profile`] must be
///   non-empty **and** [`stt_ref_usable`]. The global arm is a plain
///   `resolve_service("stt")`, which copies `provider` straight off the
///   resolved profile — so a global default pointing at an Apple-provider
///   profile reaches dictation's `"apple"` branch exactly as the widget arm
///   does, and needs the identical platform gate (not just the literal
///   `"apple-speech"` sentinel, which `resolve_profile` can't even match
///   since it's not a real `model_profiles` id).
///
/// Poisoning: because the runtime picks the widget ref before the global and
/// **never falls back** when it's set, a widget `model_profile` pointing at a
/// since-deleted profile makes STT unconfigured even when the global default is
/// perfectly valid — the dangling widget ref shadows and poisons it. Capability
/// tags are irrelevant: only the assigned default is authoritative.
pub fn is_stt_effectively_configured(config: &AppConfig) -> bool {
    let widget = stt_default_model_profile(config);
    if !widget.is_empty() {
        return stt_ref_usable(config, &widget);
    }
    !config.stt_profile.is_empty() && stt_ref_usable(config, &config.stt_profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `model_profiles` JSON entry with the given id (no capabilities needed —
    /// the STT gate is assignment-based, not capability-based).
    fn profile(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": id, "provider": "openai", "model": "m" })
    }

    /// A REAL `model_profiles` entry (a stable id, not the `"apple-speech"`
    /// sentinel) whose `provider` is `"apple"` — the shape `appleSttSeed.ts`
    /// actually seeds (`scenario-apple-stt`). Exercises the bug this fix
    /// closes: `profile_exists`-only gating passed this on Linux because it's
    /// a real `model_profiles` row, even though resolving it copies
    /// `provider: "apple"` straight into the `ServiceConfig` and lands on
    /// dictation's macOS-only branch exactly like the sentinel does.
    fn apple_profile(id: &str) -> serde_json::Value {
        serde_json::json!({ "id": id, "name": id, "provider": "apple", "model": "apple-speech" })
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
    fn widget_real_apple_provider_profile_only_counts_on_macos() {
        // Regression for the bug this fix closes: the widget's model_profile
        // is a REAL model_profiles id (not the "apple-speech" sentinel), so
        // the old profile_exists-only check passed unconditionally — even on
        // Linux, where resolving this profile still yields provider: "apple"
        // and dictation's stt_transcribe errors out.
        let cfg = cfg_with_widget_mp("scenario-apple-stt", vec![apple_profile("scenario-apple-stt")], "");
        assert_eq!(is_stt_effectively_configured(&cfg), cfg!(target_os = "macos"));
    }

    #[test]
    fn global_real_apple_provider_profile_only_counts_on_macos() {
        // Same bug, global stt_profile arm: resolve_service("stt") copies
        // provider straight off the resolved profile, so a global default
        // pointing at an Apple-provider profile reaches the exact same
        // dictation branch as the widget arm and needs the same gate.
        let cfg = AppConfig {
            model_profiles: vec![apple_profile("scenario-apple-stt")],
            stt_profile: "scenario-apple-stt".to_string(),
            ..Default::default()
        };
        assert_eq!(is_stt_effectively_configured(&cfg), cfg!(target_os = "macos"));
    }

    #[test]
    fn openai_profile_is_configured_regardless_of_platform_via_either_arm() {
        // Sanity check that the platform gate is specific to provider ==
        // "apple" and doesn't over-fire on ordinary HTTP-backed profiles.
        let widget_cfg = cfg_with_widget_mp("oa", vec![profile("oa")], "");
        assert!(is_stt_effectively_configured(&widget_cfg));

        let global_cfg = AppConfig {
            model_profiles: vec![profile("oa")],
            stt_profile: "oa".to_string(),
            ..Default::default()
        };
        assert!(is_stt_effectively_configured(&global_cfg));
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
