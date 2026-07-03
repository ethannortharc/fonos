//! Model-profile resolution: `AppConfig.model_profiles` JSON → [`ServiceConfig`].
//!
//! Pure functions over an already-loaded [`AppConfig`]; platform shells wrap
//! these with their own state/locking (e.g. the Tauri `AppState` mutex).

use crate::config::AppConfig;
use crate::llm::ServiceConfig;

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
