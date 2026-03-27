//! Skill trait and associated types for the Fonos agent system.
//!
//! A [`Skill`] is a named, executable capability that the agent can invoke in
//! response to user requests.  Skills are registered in a [`super::registry::SkillRegistry`]
//! and exposed to the LLM planner as OpenAI-style function-calling tool definitions.

use serde::{Deserialize, Serialize};

/// The result produced by a [`Skill::execute`] call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    /// Human-readable output text (always present).
    pub output: String,
    /// Optional structured data from the skill execution.
    pub structured: Option<serde_json::Value>,
}

/// A single parameter accepted by a [`Skill`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParam {
    /// Machine-readable parameter name (used as JSON key in the tool call).
    pub name: String,
    /// Human/LLM-readable description of what the parameter represents.
    pub description: String,
    /// Whether the LLM must supply this parameter.
    pub required: bool,
    /// Optional default value string (used when the parameter is omitted).
    pub default: Option<String>,
}

/// A named, executable capability that the agent can invoke.
///
/// Implementations must be `Send + Sync` so that a [`super::registry::SkillRegistry`]
/// can be shared across async tasks.
///
/// # Example
///
/// ```rust,ignore
/// use fonos_core::agent::skill::{Skill, SkillOutput, SkillParam};
/// use fonos_core::Result;
///
/// struct EchoSkill;
///
/// impl Skill for EchoSkill {
///     fn name(&self) -> &str { "echo" }
///     fn description(&self) -> &str { "Echoes the supplied text." }
///     fn parameters(&self) -> Vec<SkillParam> {
///         vec![SkillParam {
///             name: "text".into(),
///             description: "Text to echo".into(),
///             required: true,
///             default: None,
///         }]
///     }
///     async fn execute(&self, params: serde_json::Value) -> Result<SkillOutput> {
///         let text = params["text"].as_str().unwrap_or("").to_string();
///         Ok(SkillOutput { output: text, structured: None })
///     }
/// }
/// ```
pub trait Skill: Send + Sync {
    /// The unique identifier used to look up this skill by name.
    fn name(&self) -> &str;

    /// A short sentence describing what this skill does (shown to the LLM).
    fn description(&self) -> &str;

    /// The list of parameters the LLM may/must supply when invoking this skill.
    fn parameters(&self) -> Vec<SkillParam>;

    /// Execute the skill with the given JSON parameters and return the result.
    ///
    /// The future is boxed so that the trait remains dyn-compatible and skill
    /// objects can be stored as `Box<dyn Skill>` in a [`super::registry::SkillRegistry`].
    ///
    /// # Errors
    ///
    /// Returns an error if the skill fails to execute (e.g. command not found,
    /// network error, invalid parameters).
    fn execute(
        &self,
        params: serde_json::Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<SkillOutput>> + Send + '_>>;
}
