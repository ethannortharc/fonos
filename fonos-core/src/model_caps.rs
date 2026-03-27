//! Model capability detection — auto-probe LLM models and cache results.
//!
//! Capability data is persisted to `{data_dir}/com.fonos.app/model_caps.json`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{Error, Result};

/// Cached capability flags for a specific LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCaps {
    /// The model identifier as used in API calls (e.g. `"gpt-4o"`).
    pub model_id: String,
    /// Whether the model reliably follows system-prompt instructions.
    pub follows_system_prompt: bool,
    /// Whether the model preserves the input language in its output.
    pub preserves_language: bool,
    /// Unix timestamp (seconds) when these capabilities were last probed.
    pub probed_at: String,
}

/// Returns the path to the model capabilities cache file.
fn caps_path() -> PathBuf {
    dirs::data_dir()
        .expect("could not resolve data directory")
        .join("com.fonos.app")
        .join("model_caps.json")
}

/// Load all cached model capabilities from disk.
///
/// Returns an empty map if the file does not exist or cannot be parsed.
pub fn load_caps() -> HashMap<String, ModelCaps> {
    match std::fs::read_to_string(caps_path()) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

/// Persist all model capabilities to disk.
pub fn save_caps(caps: &HashMap<String, ModelCaps>) -> Result<()> {
    let path = caps_path();
    std::fs::create_dir_all(path.parent().unwrap())
        .map_err(|e| Error::Config(e.to_string()))?;
    let json = serde_json::to_string_pretty(caps)
        .map_err(|e| Error::Config(e.to_string()))?;
    std::fs::write(&path, json)
        .map_err(|e| Error::Config(format!("save caps: {e}")))
}

/// Look up cached capabilities for a specific model by ID.
///
/// Returns `None` if the model has not been probed yet.
pub fn get_caps(model_id: &str) -> Option<ModelCaps> {
    load_caps().get(model_id).cloned()
}

/// Store (or overwrite) capabilities for a single model.
pub fn store_caps(caps: ModelCaps) -> Result<()> {
    let mut all = load_caps();
    all.insert(caps.model_id.clone(), caps);
    save_caps(&all)
}

/// Build a `messages` array for an LLM API call, adapting to the model's capabilities.
///
/// If `caps` is `None` (model not yet probed), the function assumes the model follows
/// system prompts correctly. For models that do not follow system prompts, the system
/// and user content are merged into a single user message.
pub fn build_messages(
    text: &str,
    system: Option<&str>,
    user_template: &str,
    caps: Option<&ModelCaps>,
) -> Vec<serde_json::Value> {
    let user_content = user_template.replace("{text}", text);

    let follows_system = caps.map(|c| c.follows_system_prompt).unwrap_or(true);

    if follows_system {
        // Model handles system prompts well — use proper split.
        let mut msgs = Vec::new();
        if let Some(sys) = system {
            if !sys.is_empty() {
                msgs.push(serde_json::json!({"role": "system", "content": sys}));
            }
        }
        msgs.push(serde_json::json!({"role": "user", "content": user_content}));
        msgs
    } else {
        // Weak model — merge everything into user message.
        let mut combined = String::new();
        if let Some(sys) = system {
            if !sys.is_empty() {
                combined.push_str(sys);
                combined.push_str("\n\n");
            }
        }
        combined.push_str(&user_content);
        vec![serde_json::json!({"role": "user", "content": combined})]
    }
}
