//! Tauri command handlers for the Fonos agent.
//!
//! These commands bridge the frontend to the [`AgentState`] stored inside
//! [`AppState`].  Each command locks the agent mutex, performs its operation,
//! and returns a serialisable result.
//!
//! The agent field in `AppState` uses `tokio::sync::Mutex` so that the lock
//! guard can be held across `.await` points in async Tauri commands.

use serde::{Deserialize, Serialize};
use tauri::State;

use std::sync::Arc;

use fonos_core::agent::custom_loader::{load_custom_skills_typed, CustomSkillConfig};
use fonos_core::agent::processor::{AgentProcessor, AgentResult, HttpLlmCaller};
use fonos_core::agent::registry::SkillRegistry;
use fonos_core::agent::context::ConversationContext;
use fonos_core::agent::fast_path::FastPathMatcher;
use fonos_core::agent::safety::CommandSafetyFilter;
use fonos_core::llm::ServiceConfig;

use super::AppState;

// ─── SkillInfo (serialisable for the frontend) ────────────────────────────────

/// Serialisable summary of a single registered skill, returned by [`list_skills`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// Unique machine-readable skill identifier / name.
    pub id: String,
    /// Human-readable skill name (same as `id` for built-in skills).
    pub name: String,
    /// One-sentence description of what the skill does.
    pub description: String,
    /// Execution type: `"native"`, `"shell"`, `"http"`, or `"script"`.
    pub skill_type: String,
    /// Whether the skill is currently enabled in the registry.
    pub enabled: bool,
    /// `true` for built-in desktop skills; `false` for custom JSON skills.
    pub builtin: bool,
    /// Skill parameters with name, description, required, and default value.
    pub parameters: Vec<SkillParamInfo>,
}

/// Serialisable parameter info for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParamInfo {
    /// Parameter name.
    pub name: String,
    /// What the parameter does.
    pub description: String,
    /// Whether the parameter is required.
    pub required: bool,
    /// Default value, if any.
    pub default_value: Option<String>,
}

// ─── AgentState ───────────────────────────────────────────────────────────────

/// Mutable agent state held inside [`AppState`].
///
/// The registry and context are stored separately so that the processor can be
/// constructed on each call (it mutably borrows both).  The fast-path matcher
/// and system-prompt string are cheap to clone and are stored for convenience.
pub struct AgentState {
    /// The skill registry containing all registered built-in + custom skills.
    pub registry: SkillRegistry,
    /// Rolling conversation history for the current agent session.
    pub context: ConversationContext,
    /// Fast-path matcher (re-used across calls; does not hold mutable state).
    pub fast_path: FastPathMatcher,
    /// Cached system prompt from config (refreshed on each call).
    pub system_prompt: String,
    /// Cached skill execution timeout from config.
    pub timeout_secs: u64,
    /// Names of skills that were registered from built-in desktop skill code
    /// (used to populate the `builtin` field in [`SkillInfo`]).
    pub builtin_skill_names: Vec<String>,
    /// Safety filter applied to shell-type skills; re-attached when custom
    /// skills are reloaded so they stay vetted after an edit.
    pub safety: Arc<CommandSafetyFilter>,
}

impl AgentState {
    /// Create a new [`AgentState`].
    pub fn new(
        registry: SkillRegistry,
        context: ConversationContext,
        fast_path: FastPathMatcher,
        system_prompt: String,
        timeout_secs: u64,
        builtin_skill_names: Vec<String>,
        safety: Arc<CommandSafetyFilter>,
    ) -> Self {
        Self {
            registry,
            context,
            fast_path,
            system_prompt,
            timeout_secs,
            builtin_skill_names,
            safety,
        }
    }
}

// ─── Helper: convert commands::ServiceConfig -> llm::ServiceConfig ──────────

/// Convert the Tauri-layer [`super::ServiceConfig`] into the
/// [`fonos_core::llm::ServiceConfig`] expected by [`AgentProcessor`].
fn to_llm_service(sc: &super::ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        provider: sc.provider.clone(),
        api_key: sc.api_key.clone(),
        model: sc.model.clone(),
        base_url: sc.base_url.clone(),
    }
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Process a user utterance through the agent loop.
///
/// Reads the agent LLM profile from config, builds an [`AgentProcessor`] from
/// the shared state, runs it, and returns the result.
#[tauri::command]
pub async fn agent_process(
    state: State<'_, AppState>,
    text: String,
) -> Result<AgentResult, String> {
    if text.trim().is_empty() {
        return Ok(AgentResult {
            response_text: String::new(),
            skill_executions: Vec::new(),
        });
    }

    // Snapshot the config and resolve the LLM service before taking the agent lock.
    let llm_service = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        // Prefer the dedicated agent profile; fall back to the global LLM profile.
        let profile_id = if !config.agent_llm_profile.is_empty() {
            config.agent_llm_profile.clone()
        } else {
            config.llm_profile.clone()
        };
        if profile_id.is_empty() {
            return Err(
                "No LLM profile configured for the agent. Go to Settings > Agent to select a model."
                    .to_string(),
            );
        }
        let profile = config
            .model_profiles
            .iter()
            .find(|p| p["id"].as_str() == Some(&profile_id))
            .ok_or_else(|| format!("Agent LLM profile '{}' not found", profile_id))?
            .clone();
        super::config_from_profile(&profile)
    };

    let llm_service_core = to_llm_service(&llm_service);

    // Lock the agent state (tokio mutex — safe to hold across .await).
    let mut agent = state.agent.lock().await;

    // Extract the components we need to build the processor.
    // We swap out the registry, context, and fast_path so that `processor`
    // owns them, then swap them back after the call completes.
    let timeout_secs = agent.timeout_secs;
    let max_turns = timeout_secs as usize; // reuse timeout_secs as placeholder size
    let _ = max_turns; // suppress unused warning

    let registry = std::mem::replace(&mut agent.registry, SkillRegistry::new());
    // Read timeout_secs before the second replace to avoid borrow conflict.
    let context_placeholder = ConversationContext::new(agent.timeout_secs as usize);
    let context = std::mem::replace(&mut agent.context, context_placeholder);
    let fast_path = std::mem::replace(&mut agent.fast_path, FastPathMatcher::new());
    let system_prompt = agent.system_prompt.clone();

    let mut processor = AgentProcessor::<HttpLlmCaller>::new(
        registry,
        context,
        fast_path,
        system_prompt,
        timeout_secs,
    );

    let result = processor
        .process(&text, &llm_service_core)
        .await
        .map_err(|e| e.to_string());

    // Move the (potentially mutated) components back into the state.
    let (registry_back, context_back, fast_path_back) = processor.into_parts();
    agent.registry = registry_back;
    agent.context = context_back;
    agent.fast_path = fast_path_back;

    result
}

/// Reset the agent's conversation context.
#[tauri::command]
pub async fn agent_reset(state: State<'_, AppState>) -> Result<(), String> {
    let mut agent = state.agent.lock().await;
    agent.context.reset();
    Ok(())
}

/// List all registered skills with their name, description, type, and status.
#[tauri::command]
pub async fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let agent = state.agent.lock().await;
    let builtin_names = agent.builtin_skill_names.clone();
    let infos = agent
        .registry
        .list()
        .into_iter()
        .map(|si| {
            let builtin = builtin_names.contains(&si.name);
            SkillInfo {
                id: si.name.clone(),
                name: si.name.clone(),
                description: si.description.clone(),
                skill_type: if builtin { "native".to_string() } else { "custom".to_string() },
                enabled: si.enabled,
                builtin,
                parameters: si.parameters.iter().map(|p| SkillParamInfo {
                    name: p.name.clone(),
                    description: p.description.clone(),
                    required: p.required,
                    default_value: p.default.clone(),
                }).collect(),
            }
        })
        .collect();
    Ok(infos)
}

/// Enable or disable a skill by its name/ID.
#[tauri::command]
pub async fn toggle_skill(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut agent = state.agent.lock().await;
    agent.registry.toggle(&id, enabled);
    Ok(())
}

/// Save a custom skill JSON to the skills directory, then reload it into the registry.
///
/// `json_str` must be a valid [`CustomSkillConfig`] JSON string.
#[tauri::command]
pub async fn save_custom_skill(
    state: State<'_, AppState>,
    json_str: String,
) -> Result<(), String> {
    // Validate by parsing first.
    let config: CustomSkillConfig =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid skill JSON: {e}"))?;

    let skills_dir = fonos_core::config::AppConfig::config_dir().join("skills");
    std::fs::create_dir_all(&skills_dir)
        .map_err(|e| format!("Cannot create skills directory: {e}"))?;

    let filename = format!("{}.json", sanitize_skill_name(&config.name));
    let path = skills_dir.join(&filename);
    std::fs::write(&path, &json_str)
        .map_err(|e| format!("Cannot write skill file '{}': {e}", path.display()))?;

    // Reload the skill from the file and update the registry.
    let mut agent = state.agent.lock().await;
    let safety = Arc::clone(&agent.safety);
    let skills = load_custom_skills_typed(&skills_dir);
    for skill in skills {
        if skill.config.name == config.name {
            // Re-attach the safety filter so shell skills stay vetted.
            agent.registry.register(Box::new(skill.with_safety(Arc::clone(&safety))));
            break;
        }
    }

    Ok(())
}

/// Return the full stored definition of a custom skill so the UI can edit it
/// without losing its command / parameters / response template.
///
/// `list_skills` only reports a summary; this reads the skill's JSON file so an
/// edit form can be pre-filled with the real values instead of blanks.
#[tauri::command]
pub async fn get_custom_skill(id: String) -> Result<CustomSkillConfig, String> {
    let skills_dir = fonos_core::config::AppConfig::config_dir().join("skills");
    let path = skills_dir.join(format!("{}.json", sanitize_skill_name(&id)));
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read skill '{}': {e}", id))?;
    serde_json::from_str(&contents).map_err(|e| format!("Invalid skill JSON: {e}"))
}

/// Delete a custom skill by its name/ID.
///
/// Removes the JSON file from the skills directory and disables the skill
/// in the registry immediately for the current session.
#[tauri::command]
pub async fn delete_custom_skill(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let skills_dir = fonos_core::config::AppConfig::config_dir().join("skills");

    // Try to remove the JSON file.
    let filename = format!("{}.json", sanitize_skill_name(&id));
    let path = skills_dir.join(&filename);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Cannot delete skill file: {e}"))?;
    }

    // Disable the skill in the registry immediately.
    let mut agent = state.agent.lock().await;
    agent.registry.toggle(&id, false);

    Ok(())
}

/// Execute a specific skill with the given input and return its output.
///
/// `input` is JSON-encoded parameters for the skill.  If `input` is not valid
/// JSON it is treated as a plain string passed under the key `"input"`.
#[tauri::command]
pub async fn test_skill(
    state: State<'_, AppState>,
    id: String,
    input: String,
) -> Result<String, String> {
    let params: serde_json::Value = serde_json::from_str(&input).unwrap_or_else(|_| {
        serde_json::json!({ "input": input })
    });

    // Lock the agent state (tokio mutex — safe to hold across .await).
    let agent = state.agent.lock().await;
    let output = agent
        .registry
        .execute(&id, params)
        .await
        .map_err(|e| e.to_string())?;

    Ok(output.output)
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Replace characters that are not safe for filenames with underscores.
fn sanitize_skill_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
