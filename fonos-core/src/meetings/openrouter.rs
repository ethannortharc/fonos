//! OpenRouter LLM provider support for meeting summary generation.
//!
//! OpenRouter provides a unified OpenAI-compatible API at `https://openrouter.ai/api/v1`
//! that proxies many providers (Anthropic, Google, Meta, etc.) under a single endpoint.

/// Base URL for the OpenRouter API.
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Base URL for the OpenAI API (used by `resolve_provider_base_url`).
const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// List of provider IDs supported by fonos.
///
/// Includes `"openrouter"` for the OpenRouter aggregator.
pub const SUPPORTED_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "openrouter",
    "ollama",
    "lmstudio",
    "custom",
];

/// Return the base URL for the OpenRouter API.
///
/// Always returns `"https://openrouter.ai/api/v1"`.
pub fn openrouter_base_url() -> &'static str {
    OPENROUTER_BASE_URL
}

/// Resolve the API base URL for a named provider.
///
/// | Provider      | Base URL                           |
/// |---------------|-------------------------------------|
/// | `openrouter`  | `https://openrouter.ai/api/v1`      |
/// | `openai`      | `https://api.openai.com/v1`         |
/// | anything else | provider string passed through as-is |
pub fn resolve_provider_base_url(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "openrouter" => OPENROUTER_BASE_URL.to_string(),
        "openai" => OPENAI_BASE_URL.to_string(),
        other => other.to_string(),
    }
}

/// Check whether `model_id` is a valid OpenRouter-namespaced model identifier.
///
/// OpenRouter model IDs follow the `"<provider>/<model>"` convention, e.g.:
/// - `"anthropic/claude-sonnet-4"`
/// - `"google/gemini-2.5-flash"`
///
/// Plain model names like `"gpt-4o"` (no `/`) are not valid OpenRouter IDs.
pub fn is_valid_openrouter_model_id(model_id: &str) -> bool {
    // Must contain exactly one `/` separator and both parts must be non-empty.
    let parts: Vec<&str> = model_id.splitn(2, '/').collect();
    parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_base_url_correct() {
        assert_eq!(openrouter_base_url(), "https://openrouter.ai/api/v1");
    }

    #[test]
    fn resolve_openrouter() {
        assert_eq!(
            resolve_provider_base_url("openrouter"),
            "https://openrouter.ai/api/v1"
        );
    }

    #[test]
    fn valid_model_ids() {
        assert!(is_valid_openrouter_model_id("anthropic/claude-sonnet-4"));
        assert!(is_valid_openrouter_model_id("google/gemini-2.5-flash"));
    }

    #[test]
    fn invalid_model_ids() {
        assert!(!is_valid_openrouter_model_id("gpt-4o"));
        assert!(!is_valid_openrouter_model_id(""));
    }
}
