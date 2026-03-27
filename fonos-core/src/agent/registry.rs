//! Skill registry — stores and manages the set of available [`Skill`] implementations.
//!
//! The [`SkillRegistry`] is the central hub for skill management in the Fonos agent:
//! skills are registered at startup (built-in + custom), can be toggled on/off at
//! runtime, and can be serialised into OpenAI-style tool definitions for inclusion
//! in LLM requests.

use std::collections::HashMap;

use serde_json::json;

use super::skill::{Skill, SkillOutput};

/// Runtime state tracked per registered skill.
struct SkillEntry {
    /// The skill implementation (boxed so the registry is object-safe).
    skill: Box<dyn Skill>,
    /// Whether this skill is currently enabled.
    enabled: bool,
}

/// Summary information about a registered skill, returned by [`SkillRegistry::list`].
#[derive(Debug, Clone)]
pub struct SkillInfo {
    /// Unique skill name.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Whether the skill is currently enabled.
    pub enabled: bool,
    /// The parameters this skill accepts.
    pub parameters: Vec<crate::agent::skill::SkillParam>,
}

/// Manages the set of available [`Skill`] implementations at runtime.
///
/// Skills are stored by name and can be individually enabled or disabled without
/// being removed from the registry.  The registry also generates the `tools`
/// array that is sent to the LLM planner in every request.
///
/// # Thread safety
///
/// `SkillRegistry` itself is not `Sync`; callers that need shared mutable access
/// across threads should wrap it in `Arc<Mutex<SkillRegistry>>`.
pub struct SkillRegistry {
    /// Ordered list of skills (insertion order preserved for stable tool indices).
    entries: Vec<SkillEntry>,
    /// Name-to-index mapping for O(1) lookup.
    index: HashMap<String, usize>,
}

impl SkillRegistry {
    /// Create a new, empty [`SkillRegistry`].
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Register a skill.
    ///
    /// If a skill with the same name is already registered it is replaced.
    /// Newly registered skills are **enabled** by default.
    pub fn register(&mut self, skill: Box<dyn Skill>) {
        let name = skill.name().to_string();
        if let Some(&idx) = self.index.get(&name) {
            // Replace in-place, preserving position and enabled state.
            self.entries[idx].skill = skill;
        } else {
            let idx = self.entries.len();
            self.index.insert(name, idx);
            self.entries.push(SkillEntry { skill, enabled: true });
        }
    }

    /// Look up a skill by name.
    ///
    /// Returns `None` if the skill is not registered (regardless of enabled state).
    pub fn get(&self, name: &str) -> Option<&dyn Skill> {
        self.index.get(name).map(|&idx| self.entries[idx].skill.as_ref())
    }

    /// Returns `true` if the named skill is registered **and** currently enabled.
    pub fn is_enabled(&self, name: &str) -> bool {
        self.index
            .get(name)
            .map(|&idx| self.entries[idx].enabled)
            .unwrap_or(false)
    }

    /// Return summary information for every registered skill (enabled or not).
    ///
    /// The order matches registration order.
    pub fn list(&self) -> Vec<SkillInfo> {
        self.entries
            .iter()
            .map(|e| SkillInfo {
                name: e.skill.name().to_string(),
                description: e.skill.description().to_string(),
                enabled: e.enabled,
                parameters: e.skill.parameters(),
            })
            .collect()
    }

    /// Enable or disable a skill by name.
    ///
    /// Has no effect if the skill is not registered.
    pub fn toggle(&mut self, name: &str, enabled: bool) {
        if let Some(&idx) = self.index.get(name) {
            self.entries[idx].enabled = enabled;
        }
    }

    /// Generate the OpenAI function-calling `tools` array for all **enabled** skills.
    ///
    /// Each element has the shape:
    ///
    /// ```json
    /// {
    ///   "type": "function",
    ///   "function": {
    ///     "name": "<skill name>",
    ///     "description": "<skill description>",
    ///     "parameters": {
    ///       "type": "object",
    ///       "properties": { ... },
    ///       "required": [ ... ]
    ///     }
    ///   }
    /// }
    /// ```
    pub fn tool_definitions(&self) -> Vec<serde_json::Value> {
        self.entries
            .iter()
            .filter(|e| e.enabled)
            .map(|e| {
                let params = e.skill.parameters();

                let mut properties = serde_json::Map::new();
                let mut required: Vec<serde_json::Value> = Vec::new();

                for p in &params {
                    let mut prop = serde_json::Map::new();
                    prop.insert("type".into(), json!("string"));
                    prop.insert("description".into(), json!(p.description));
                    if let Some(ref default) = p.default {
                        prop.insert("default".into(), json!(default));
                    }
                    properties.insert(p.name.clone(), serde_json::Value::Object(prop));

                    if p.required {
                        required.push(json!(p.name));
                    }
                }

                json!({
                    "type": "function",
                    "function": {
                        "name": e.skill.name(),
                        "description": e.skill.description(),
                        "parameters": {
                            "type": "object",
                            "properties": properties,
                            "required": required,
                        }
                    }
                })
            })
            .collect()
    }

    /// Execute a skill by name with the given parameters.
    ///
    /// Returns an error if the skill is not found, is disabled, or fails during
    /// execution.
    pub async fn execute(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> crate::Result<SkillOutput> {
        let idx = self
            .index
            .get(name)
            .copied()
            .ok_or_else(|| crate::Error::Agent(format!("skill not found: {name}")))?;

        let entry = &self.entries[idx];
        if !entry.enabled {
            return Err(crate::Error::Agent(format!("skill is disabled: {name}")));
        }

        entry.skill.execute(params).await
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Unit Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::skill::{Skill, SkillOutput, SkillParam};

    /// A minimal mock skill used in registry tests.
    struct MockEchoSkill {
        name: &'static str,
        description: &'static str,
    }

    impl Skill for MockEchoSkill {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            self.description
        }
        fn parameters(&self) -> Vec<SkillParam> {
            vec![SkillParam {
                name: "text".into(),
                description: "Text to echo back".into(),
                required: true,
                default: None,
            }]
        }
        fn execute(
            &self,
            params: serde_json::Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<SkillOutput>> + Send + '_>> {
            let text = params["text"].as_str().unwrap_or("").to_string();
            Box::pin(async move {
                Ok(SkillOutput {
                    output: text,
                    structured: None,
                })
            })
        }
    }

    /// Another mock skill with an optional parameter, to test tool_definitions.
    struct MockShellSkill;

    impl Skill for MockShellSkill {
        fn name(&self) -> &str {
            "shell"
        }
        fn description(&self) -> &str {
            "Run a shell command and return its output."
        }
        fn parameters(&self) -> Vec<SkillParam> {
            vec![
                SkillParam {
                    name: "command".into(),
                    description: "Shell command to execute".into(),
                    required: true,
                    default: None,
                },
                SkillParam {
                    name: "timeout_secs".into(),
                    description: "Override the execution timeout in seconds".into(),
                    required: false,
                    default: Some("30".into()),
                },
            ]
        }
        fn execute(
            &self,
            params: serde_json::Value,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<SkillOutput>> + Send + '_>> {
            let cmd = params["command"].as_str().unwrap_or("").to_string();
            Box::pin(async move {
                Ok(SkillOutput {
                    output: format!("ran: {cmd}"),
                    structured: None,
                })
            })
        }
    }

    fn make_registry() -> SkillRegistry {
        let mut reg = SkillRegistry::new();
        reg.register(Box::new(MockShellSkill));
        reg.register(Box::new(MockEchoSkill {
            name: "echo",
            description: "Echoes text back.",
        }));
        reg
    }

    // ── INV-01 tests ─────────────────────────────────────────────────────────

    /// Register two skills; get returns the right one by name.
    #[test]
    fn test_skill_registry_get_known() {
        let reg = make_registry();
        assert!(reg.get("shell").is_some());
        assert!(reg.get("echo").is_some());
    }

    /// get with an unknown name returns None.
    #[test]
    fn test_skill_registry_get_nonexistent() {
        let reg = make_registry();
        assert!(reg.get("nonexistent").is_none());
    }

    /// list returns both registered skills with correct descriptions.
    #[test]
    fn test_skill_registry_list() {
        let reg = make_registry();
        let list = reg.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "shell");
        assert_eq!(list[1].name, "echo");
        assert!(list[0].description.contains("shell command"));
        assert!(list[1].description.contains("Echoes"));
    }

    /// All skills are enabled by default; toggle disables/re-enables correctly.
    #[test]
    fn test_skill_registry_toggle() {
        let mut reg = make_registry();

        // Both enabled initially.
        assert!(reg.is_enabled("shell"));
        assert!(reg.is_enabled("echo"));

        // Disable echo.
        reg.toggle("echo", false);
        assert!(reg.is_enabled("shell"));
        assert!(!reg.is_enabled("echo"));

        // Re-enable.
        reg.toggle("echo", true);
        assert!(reg.is_enabled("echo"));

        // Toggling a non-existent skill is a no-op.
        reg.toggle("nonexistent", false);
    }

    /// tool_definitions returns one entry per *enabled* skill in OpenAI format.
    #[test]
    fn test_skill_registry_tool_definitions_structure() {
        let mut reg = make_registry();

        let defs = reg.tool_definitions();
        assert_eq!(defs.len(), 2, "both skills enabled");

        let shell_def = &defs[0];
        assert_eq!(shell_def["type"], "function");
        assert_eq!(shell_def["function"]["name"], "shell");
        assert!(shell_def["function"]["description"].as_str().unwrap().len() > 0);

        // "command" is required; "timeout_secs" is not.
        let required = &shell_def["function"]["parameters"]["required"];
        assert!(required.as_array().unwrap().contains(&serde_json::json!("command")));
        assert!(!required.as_array().unwrap().contains(&serde_json::json!("timeout_secs")));

        // Disable shell — should now only return one definition.
        reg.toggle("shell", false);
        let defs = reg.tool_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0]["function"]["name"], "echo");
    }

    /// Execute a known, enabled skill returns the expected output.
    #[tokio::test]
    async fn test_skill_registry_execute_ok() {
        let reg = make_registry();
        let result = reg
            .execute("echo", serde_json::json!({"text": "hello agent"}))
            .await
            .expect("execute should succeed");
        assert_eq!(result.output, "hello agent");
        assert!(result.structured.is_none());
    }

    /// Execute an unknown skill returns an error.
    #[tokio::test]
    async fn test_skill_registry_execute_unknown() {
        let reg = make_registry();
        let err = reg
            .execute("nonexistent", serde_json::json!({}))
            .await
            .expect_err("should fail for unknown skill");
        assert!(err.to_string().contains("not found"));
    }

    /// Execute a disabled skill returns an error.
    #[tokio::test]
    async fn test_skill_registry_execute_disabled() {
        let mut reg = make_registry();
        reg.toggle("echo", false);
        let err = reg
            .execute("echo", serde_json::json!({"text": "hi"}))
            .await
            .expect_err("should fail for disabled skill");
        assert!(err.to_string().contains("disabled"));
    }

    /// Registering a skill with a duplicate name replaces it.
    #[test]
    fn test_skill_registry_replace_duplicate() {
        let mut reg = SkillRegistry::new();
        reg.register(Box::new(MockEchoSkill {
            name: "echo",
            description: "first",
        }));
        reg.register(Box::new(MockEchoSkill {
            name: "echo",
            description: "second",
        }));
        let list = reg.list();
        assert_eq!(list.len(), 1, "duplicate should replace, not add");
        assert_eq!(list[0].description, "second");
    }

    /// Default registry is empty.
    #[test]
    fn test_skill_registry_default_empty() {
        let reg = SkillRegistry::default();
        assert!(reg.list().is_empty());
        assert!(reg.tool_definitions().is_empty());
    }
}
