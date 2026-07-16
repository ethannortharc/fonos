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
/// constructed on each call (it mutably borrows both). The fast-path matcher
/// is cheap to clone and is stored for convenience.
///
/// **No `system_prompt` field** (final review wave, I1 — removed): it used to
/// cache `config.agent_system_prompt` ONCE at startup (`main.rs`'s
/// `AgentState::new` construction), so editing the `agent.default` widget's
/// persona in the Workbench never reached [`agent_process`]'s typed/mic path
/// — only the voice (`commands::agent_widget::run_agent_exchange`) path saw
/// it, because that path always resolved its persona fresh per call. Both
/// callers of [`run_agent_processor`] now resolve a fresh persona on every
/// call (`agent_process` via
/// `commands::agent_widget::resolve_agent_default_persona`), so there is no
/// remaining reader for a cached copy.
pub struct AgentState {
    /// The skill registry containing all registered built-in + custom skills.
    pub registry: SkillRegistry,
    /// Rolling conversation history for the current agent session.
    pub context: ConversationContext,
    /// Fast-path matcher (re-used across calls; does not hold mutable state).
    pub fast_path: FastPathMatcher,
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
        timeout_secs: u64,
        builtin_skill_names: Vec<String>,
        safety: Arc<CommandSafetyFilter>,
    ) -> Self {
        Self {
            registry,
            context,
            fast_path,
            timeout_secs,
            builtin_skill_names,
            safety,
        }
    }
}

// ─── Shared helpers (also used by commands::agent_widget's composite path) ────

/// Resolve `profile_id` into a concrete LLM [`ServiceConfig`], with the
/// agent's own user-facing error copy for the two ways this can fail: no
/// profile configured at all, or a configured id that no longer exists among
/// `config.model_profiles`. Shared by [`agent_process`]'s config-only
/// resolution (`agent_llm_profile`→`llm_profile`) and
/// `commands::agent_widget::run_agent_exchange`'s ref-or-fallback resolution
/// (Workbench P2 Task 6), so both paths report the same two failure modes
/// identically.
pub(crate) fn resolve_agent_llm_service(
    config: &fonos_core::config::AppConfig,
    profile_id: &str,
) -> Result<ServiceConfig, String> {
    if profile_id.is_empty() {
        // I2 fix (final review wave): the old copy pointed at "Settings >
        // Agent", a page Workbench P2 deleted (AgentTab.tsx) — model
        // selection now lives on the agent widget's own props form.
        return Err(
            "No LLM profile configured for the agent. Configure the Agent widget's LLM / 在智能体组件中配置 LLM."
                .to_string(),
        );
    }
    let profile = config
        .model_profiles
        .iter()
        .find(|p| p["id"].as_str() == Some(profile_id))
        .ok_or_else(|| format!("Agent LLM profile '{}' not found", profile_id))?
        .clone();
    Ok(super::config_from_profile(&profile))
}

/// Run the agent's skill loop over `text` using `llm_service`, swapping the
/// shared [`AgentState`]'s registry/context/fast_path out for the duration of
/// the call and back in afterward (so `processor` can own them mutably), then
/// returning the result.
///
/// `timeout_override` replaces `agent.timeout_secs` for this call only when
/// `Some` — Workbench P2 Task 6's per-widget `AgentProps::timeout_secs`;
/// `None` keeps using the shared state's own value, which is exactly
/// [`agent_process`]'s prior (pre-extraction) behavior.
///
/// `system_prompt` is the already-resolved persona for this call — Fix Round
/// 1 introduced this as `AgentProps::system`, resolved by
/// `commands::agent_widget::run_agent_exchange` for the voice path; I1 (final
/// review wave) closed the seam where [`agent_process`] instead used a
/// startup-cached `AgentState.system_prompt` that never reflected later edits
/// to the `agent.default` widget — both callers now resolve fresh every call
/// (`agent_process` via
/// `commands::agent_widget::resolve_agent_default_persona`), so this
/// parameter is mandatory rather than an `Option` with a stale-state fallback.
pub(crate) async fn run_agent_processor(
    state: &State<'_, AppState>,
    text: &str,
    llm_service: ServiceConfig,
    timeout_override: Option<u64>,
    system_prompt: String,
) -> Result<AgentResult, String> {
    // Lock the agent state (tokio mutex — safe to hold across .await).
    let mut agent = state.agent.lock().await;

    let timeout_secs = timeout_override.unwrap_or(agent.timeout_secs);

    // Extract the components we need to build the processor.
    // We swap out the registry, context, and fast_path so that `processor`
    // owns them, then swap them back after the call completes.
    let registry = std::mem::replace(&mut agent.registry, SkillRegistry::new());
    // Read timeout_secs before the second replace to avoid borrow conflict.
    let context_placeholder = ConversationContext::new(agent.timeout_secs as usize);
    let context = std::mem::replace(&mut agent.context, context_placeholder);
    let fast_path = std::mem::replace(&mut agent.fast_path, FastPathMatcher::new());

    let mut processor = AgentProcessor::<HttpLlmCaller>::new(
        registry,
        context,
        fast_path,
        system_prompt,
        timeout_secs,
    );

    let result = processor
        .process(text, &llm_service)
        .await
        .map_err(|e| e.to_string());

    // Move the (potentially mutated) components back into the state.
    let (registry_back, context_back, fast_path_back) = processor.into_parts();
    agent.registry = registry_back;
    agent.context = context_back;
    agent.fast_path = fast_path_back;

    result
}

// ─── Tauri commands ───────────────────────────────────────────────────────────

/// Process a user utterance through the agent loop.
///
/// Reads the agent LLM profile from config, builds an [`AgentProcessor`] from
/// the shared state, runs it, and returns the result.
///
/// The model resolution is unchanged (`agent_llm_profile`→`llm_profile`
/// config chain). The persona (I1 fix, final review wave) now resolves the
/// same way `commands::agent_widget::run_agent_exchange` does for a
/// widget-sourced call — via
/// `agent_widget::resolve_agent_default_persona` against the singleton
/// `agent.default` widget — instead of the removed `AgentState.system_prompt`
/// startup cache, so editing that widget's persona in the Workbench takes
/// effect here immediately, without an app restart.
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

    // Snapshot the config and resolve the LLM service + persona before taking
    // the agent lock.
    let (llm_service, system_prompt) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        // Prefer the dedicated agent profile; fall back to the global LLM profile.
        let profile_id = if !config.agent_llm_profile.is_empty() {
            config.agent_llm_profile.clone()
        } else {
            config.llm_profile.clone()
        };
        let llm_service = resolve_agent_llm_service(&config, &profile_id)?;

        let widgets = fonos_core::workflow::engine::effective_widgets(&config);
        let system_prompt = super::agent_widget::resolve_agent_default_persona(&widgets)?;
        (llm_service, system_prompt)
    };

    run_agent_processor(&state, &text, llm_service, None, system_prompt).await
}

/// Reset the agent's conversation context.
#[tauri::command]
pub async fn agent_reset(state: State<'_, AppState>) -> Result<(), String> {
    let mut agent = state.agent.lock().await;
    agent.context.reset();
    Ok(())
}

/// Pure core of [`agent_llm_provider`] (issue #69): does **any** resolution
/// path a shared-agent-panel exchange might take land on the Anthropic
/// provider? Takes `&AppConfig` (not a Tauri `State`) so it is unit-testable.
///
/// Two kinds of exchange surface in that one panel, resolved *differently*, so
/// an honest notice must consider both:
///
/// - **The panel's own typed/mic buttons** call [`agent_process`], whose
///   *model* is resolved purely from the legacy `agent_llm_profile`→
///   `llm_profile` config chain — it never consults any widget's `llm_widget`.
///   Checked as path (b).
/// - **The voice recipe and any workflow-wired agent widget** run through
///   `commands::agent_widget::run_agent_exchange`, whose model comes from the
///   *triggering* `agent`-type widget's own `llm_widget` ref (falling through
///   to that same legacy chain when the ref is absent or its `model_profile`
///   is empty). Checked as path (a), over **every** effective agent widget —
///   `agent.default` *and* any custom agent widgets a user wired into a
///   workflow, not just the hardcoded singleton the first cut checked.
///
/// `agent::processor::HttpLlmCaller`'s Anthropic arm drops the `tools` array,
/// so an Anthropic-backed exchange still chats but silently runs no skills;
/// the panel shows a "chat-only" notice whenever this returns `true`. Firing
/// on *any* Anthropic-reaching path — rather than mirroring only
/// `run_agent_exchange`'s widget-ref chain, as the first cut did — closes both
/// a false negative (the legacy chain on Anthropic while the widget ref points
/// elsewhere silently reproduced #69 for the panel's typed/mic path) and a
/// false positive.
///
/// A **dangling or mistyped `llm_widget` ref is skipped**, *not* treated as a
/// fall-through to the legacy chain (the behaviour the first cut's
/// `.unwrap_or(None)` produced, contradicting its own doc): `run_agent_exchange`
/// hard-errors on such a ref — surfacing its own message the instant the user
/// sends — so that widget can never *silently* reach Anthropic, and counting
/// it would misreport a send that will visibly fail. The panel's typed/mic
/// path is unaffected by a dangling widget ref; path (b) covers it
/// independently.
///
/// Read-only and never panics: every resolution failure (missing/unconfigured
/// profile, unparseable props) counts as "not Anthropic" for that path rather
/// than aborting this display-only lookup — the real send path still surfaces
/// the true error once the user actually sends.
pub(crate) fn agent_any_anthropic(config: &fonos_core::config::AppConfig) -> bool {
    let widgets = fonos_core::workflow::engine::effective_widgets(config);

    // A profile id resolves to Anthropic? Resolution failure ⇒ "not Anthropic"
    // (never blocks — this is a display-only lookup).
    let hits_anthropic = |profile_id: &str| -> bool {
        resolve_agent_llm_service(config, profile_id)
            .map(|svc| svc.provider == "anthropic")
            .unwrap_or(false)
    };

    // (a) Every agent-type widget's own model resolution, exactly as
    // `run_agent_exchange` would resolve it (ref → tier → provider).
    let widget_hit = widgets
        .iter()
        .filter(|w| w.type_tag == "agent")
        .any(|w| {
            let Ok(props) =
                serde_json::from_value::<super::agent_widget::AgentProps>(w.props.clone())
            else {
                return false; // unparseable props: the send path can't run this widget
            };
            // Dangling / mistyped ref: skip. `run_agent_exchange` hard-errors
            // on it, so it never silently reaches Anthropic (see doc above).
            let Ok(ref_payload) =
                super::agent_widget::resolve_agent_llm_widget_ref(&props, &widgets)
            else {
                return false;
            };
            let (profile_id, _persona) = super::agent_widget::resolve_agent_llm_tier(
                ref_payload,
                &config.agent_llm_profile,
                &config.llm_profile,
                &props.system,
            );
            hits_anthropic(&profile_id)
        });
    if widget_hit {
        return true;
    }

    // (b) The legacy chain `agent_process` uses for the panel's typed/mic sends.
    let legacy_profile = if !config.agent_llm_profile.is_empty() {
        config.agent_llm_profile.as_str()
    } else {
        config.llm_profile.as_str()
    };
    hits_anthropic(legacy_profile)
}

/// Report whether the shared agent panel should show issue #69's "chat-only,
/// no tools" notice: returns `"anthropic"` when **any** resolution path an
/// agent exchange might take lands on the Anthropic provider, `""` otherwise.
///
/// Thin `State`-bound wrapper over [`agent_any_anthropic`], which carries the
/// full contract (what "any path" means, why dangling refs are skipped, and
/// why it never hard-errors). Kept returning a provider-ish string rather than
/// a bool so the panel's existing `p === 'anthropic'` check — and this
/// command's `generate_handler!` registration name — are both unchanged.
#[tauri::command]
pub fn agent_llm_provider(state: State<'_, AppState>) -> Result<String, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(if agent_any_anthropic(&config) {
        "anthropic".to_string()
    } else {
        String::new()
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use fonos_core::config::AppConfig;
    use fonos_core::workflow::model::{WidgetDef, WidgetRole};

    /// A model profile carrying an explicit `provider` — `config_from_profile`
    /// reads `provider` straight off this JSON (see `services::service_from_profile`).
    fn profile(id: &str, provider: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": id,
            "provider": provider,
            "base_url": "http://x.example",
            "api_key": "",
            "model": "m",
            "capabilities": ["llm"],
        })
    }

    /// A tuned `llm` widget pointing at `model_profile`.
    fn llm_widget(id: &str, model_profile: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Processor,
            type_tag: "llm".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "model_profile": model_profile }),
            builtin: false,
        }
    }

    /// An `agent`-type widget wired to `llm_widget` (empty ⇒ legacy chain).
    /// Pushed into `config.widgets`: id `"agent.default"` REPLACES the builtin
    /// (overlay-by-id); any other id APPENDS a custom agent widget.
    fn agent_widget(id: &str, llm_widget_ref: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            role: WidgetRole::Output,
            type_tag: "agent".to_string(),
            name: id.to_string(),
            icon: String::new(),
            props: serde_json::json!({ "llm_widget": llm_widget_ref }),
            builtin: false,
        }
    }

    #[test]
    fn default_config_is_not_anthropic() {
        // Builtin agent.default ships llm_widget:"" and the default legacy
        // chain resolves to no anthropic profile.
        assert!(!agent_any_anthropic(&AppConfig::default()));
    }

    #[test]
    fn legacy_chain_anthropic_fires_even_when_widget_ref_points_elsewhere() {
        // agent_process (the panel's typed/mic path) resolves its model ONLY
        // from the legacy chain, so an Anthropic llm_profile must fire the
        // notice even though agent.default's llm_widget ref resolves to a
        // NON-anthropic profile. The first cut mirrored only the widget ref
        // and returned "" here — finding 1's silent #69.
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(profile("anthropic-llm", "anthropic"));
        cfg.model_profiles.push(profile("local-llm", "openai"));
        cfg.agent_llm_profile = String::new();
        cfg.llm_profile = "anthropic-llm".into();
        cfg.widgets.push(agent_widget("agent.default", "llm.local"));
        cfg.widgets.push(llm_widget("llm.local", "local-llm"));
        assert!(agent_any_anthropic(&cfg));
    }

    #[test]
    fn custom_agent_widget_ref_anthropic_fires() {
        // A user-created agent widget (finding 2) wired into a workflow resolves
        // to Anthropic even though agent.default and the legacy chain do not.
        // The first cut hardcoded "agent.default" and missed this.
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(profile("anthropic-llm", "anthropic"));
        cfg.model_profiles.push(profile("local-llm", "openai"));
        cfg.agent_llm_profile = "local-llm".into();
        cfg.llm_profile = "local-llm".into();
        cfg.widgets.push(agent_widget("agent.default", "llm.local"));
        cfg.widgets.push(llm_widget("llm.local", "local-llm"));
        cfg.widgets.push(agent_widget("agent.custom", "llm.claude"));
        cfg.widgets.push(llm_widget("llm.claude", "anthropic-llm"));
        assert!(agent_any_anthropic(&cfg));
    }

    #[test]
    fn agent_default_ref_anthropic_fires() {
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(profile("anthropic-llm", "anthropic"));
        cfg.llm_profile = "local-llm".into(); // absent id ⇒ legacy path resolves to nothing
        cfg.widgets.push(agent_widget("agent.default", "llm.claude"));
        cfg.widgets.push(llm_widget("llm.claude", "anthropic-llm"));
        assert!(agent_any_anthropic(&cfg));
    }

    #[test]
    fn dangling_widget_ref_is_skipped_not_treated_as_fallthrough() {
        // A dangling llm_widget ref (finding 4): run_agent_exchange hard-errors
        // on it, so it must NOT contribute an Anthropic hit. With the legacy
        // chain non-Anthropic, the notice stays off — the first cut swallowed
        // the Err and fell through to the legacy chain, contradicting its doc.
        // Also proves error-tolerance: the resolution failure never panics.
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(profile("anthropic-llm", "anthropic"));
        cfg.model_profiles.push(profile("local-llm", "openai"));
        cfg.agent_llm_profile = "local-llm".into();
        cfg.llm_profile = "local-llm".into();
        cfg.widgets.push(agent_widget("agent.default", "llm.missing"));
        assert!(!agent_any_anthropic(&cfg));
    }

    #[test]
    fn dangling_widget_ref_does_not_hide_anthropic_legacy_chain() {
        // Even with agent.default's ref dangling (its voice path would error),
        // the panel's typed/mic path still uses the legacy chain — so an
        // Anthropic legacy chain must fire. Path (b) is independent of path (a).
        let mut cfg = AppConfig::default();
        cfg.model_profiles.push(profile("anthropic-llm", "anthropic"));
        cfg.agent_llm_profile = "anthropic-llm".into();
        cfg.llm_profile = "anthropic-llm".into();
        cfg.widgets.push(agent_widget("agent.default", "llm.missing"));
        assert!(agent_any_anthropic(&cfg));
    }

    #[test]
    fn unresolvable_profile_counts_as_not_anthropic() {
        // Refs resolve OK but point at absent profile ids ⇒ resolution fails ⇒
        // "not Anthropic", never a panic (error-tolerant, never blocks).
        let mut cfg = AppConfig::default();
        cfg.llm_profile = "ghost-profile".into();
        cfg.widgets.push(agent_widget("agent.default", "llm.ghost"));
        cfg.widgets.push(llm_widget("llm.ghost", "also-ghost"));
        assert!(!agent_any_anthropic(&cfg));
    }
}
