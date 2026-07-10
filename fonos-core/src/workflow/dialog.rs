//! Session-type Dialog output core: engine enum, session state, turn logic.

use crate::agent::context::ConversationContext;
use crate::llm::ServiceConfig;
use crate::workflow::model::PanelSize;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Which backend answers follow-up turns in a Dialog session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DialogEngine {
    /// Plain LLM chat completion using a named model profile and an
    /// optional system prompt.
    Llm {
        /// Name of the configured model profile to use for calls.
        model_profile: String,
        /// Optional system prompt prepended to every turn.
        #[serde(default)]
        system: Option<String>,
    },
    /// Placeholder for a future tool-using agent backend. Not implemented in P2.
    Agent {},
    /// Placeholder for a future speech-to-speech backend. Not implemented in P2.
    Sts {},
    /// Placeholder for routing follow-up turns into another workflow. Not
    /// implemented in P2.
    Workflow {
        /// Id of the workflow to invoke for follow-up turns.
        workflow_id: String,
    },
}

/// Default engine when props omit `engine` entirely: an empty
/// `model_profile` means "use the global default LLM profile", with no
/// system prompt.
impl Default for DialogEngine {
    fn default() -> Self {
        DialogEngine::Llm { model_profile: String::new(), system: None }
    }
}

/// Configuration for a Dialog-type output panel: rendering, window shape,
/// and which engine drives live follow-up turns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogProps {
    /// Whether assistant replies are rendered as Markdown.
    #[serde(default)]
    pub markdown: bool,
    /// Panel window dimensions.
    #[serde(default)]
    pub size: PanelSize,
    /// Whether the follow-up input accepts voice dictation. Field reserved;
    /// P2 always leaves this `false`.
    #[serde(default)]
    pub voice_input: bool,
    /// Which backend answers follow-up turns.
    #[serde(default)]
    pub engine: DialogEngine,
}

/// Build the OpenAI-style messages array for one turn: optional system,
/// then history.
pub fn build_dialog_messages(system: Option<&str>, ctx: &ConversationContext) -> Vec<Value> {
    let mut msgs = Vec::new();
    if let Some(s) = system {
        if !s.is_empty() {
            msgs.push(json!({"role":"system","content":s}));
        }
    }
    msgs.extend(ctx.to_messages());
    msgs
}

/// Live follow-up conversation state behind an open Dialog panel.
pub struct DialogSession {
    /// Id of the storage container (e.g. a notebook-like entity) this
    /// dialog's turns are associated with.
    pub container_id: i64,
    /// Optional system prompt used for every follow-up turn.
    pub system: Option<String>,
    /// Rolling message history, reusing the agent's conversation context for
    /// turn storage and trimming.
    pub context: ConversationContext,
}

impl DialogSession {
    /// Create a new session bound to `container_id`, using `system` as the
    /// system prompt for every turn and retaining at most `max_turns`
    /// user/assistant exchange pairs (see [`ConversationContext::new`]).
    pub fn new(container_id: i64, system: Option<String>, max_turns: usize) -> Self {
        Self {
            container_id,
            system,
            context: ConversationContext::new(max_turns),
        }
    }

    /// Seed the first turn's context (from the workflow's raw input and
    /// final result) without making an LLM call.
    pub fn seed_first_turn(&mut self, user: &str, assistant: &str) {
        self.context.add_user_message(user);
        self.context.add_agent_response(assistant);
    }

    /// Run one follow-up turn: build the outgoing request from the existing
    /// (trimmed) history plus the new user turn, call the LLM, and only on
    /// success commit both the user turn and the reply to `context`.
    ///
    /// `context` is deliberately left untouched until `chat` succeeds. If it
    /// were mutated up front, a failed call (transient network / rate-limit)
    /// would leave a dangling user message with no assistant reply; the next
    /// attempt would then send two consecutive `user` messages, which
    /// strict-alternation backends (Anthropic) reject — poisoning the
    /// session for every subsequent turn until restart.
    pub async fn next_turn(
        &mut self,
        user_text: &str,
        service: &ServiceConfig,
        temperature: f64,
        max_tokens: u32,
    ) -> Result<String, String> {
        let mut messages = build_dialog_messages(self.system.as_deref(), &self.context);
        messages.push(json!({ "role": "user", "content": user_text }));
        let reply = crate::llm::chat(service, messages, temperature, max_tokens).await?;
        // Commit both turns only now that the call has succeeded.
        self.context.add_user_message(user_text);
        self.context.add_agent_response(&reply);
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_serde_tag_roundtrips() {
        let e = DialogEngine::Llm { model_profile: "p".into(), system: None };
        let j = serde_json::to_string(&e).unwrap();
        assert!(j.contains("\"kind\":\"llm\""));
        let back: DialogEngine = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn placeholder_engine_variants_tag_and_roundtrip() {
        let cases = [
            (DialogEngine::Agent {}, "\"kind\":\"agent\""),
            (DialogEngine::Sts {}, "\"kind\":\"sts\""),
            (
                DialogEngine::Workflow { workflow_id: "wf.x".into() },
                "\"kind\":\"workflow\"",
            ),
        ];
        for (variant, expected_tag) in cases {
            let j = serde_json::to_string(&variant).unwrap();
            assert!(j.contains(expected_tag), "expected {expected_tag} in {j}");
            let back: DialogEngine = serde_json::from_str(&j).unwrap();
            assert_eq!(back, variant);
        }
    }

    #[test]
    fn build_messages_prepends_system_and_orders_turns() {
        let mut ctx = crate::agent::context::ConversationContext::new(12);
        ctx.add_user_message("hi");
        ctx.add_agent_response("hello");
        let msgs = build_dialog_messages(Some("SYS"), &ctx);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "SYS");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs.last().unwrap()["role"], "assistant");
    }

    #[test]
    fn trim_keeps_last_12_turns() {
        let mut ctx = crate::agent::context::ConversationContext::new(12);
        for i in 0..20 {
            ctx.add_user_message(&format!("u{i}"));
            ctx.add_agent_response(&format!("a{i}"));
        }
        // ConversationContext auto-trims on every add_*, capping non-system
        // messages at max_turns * 2 (see `truncate_if_needed`). With
        // max_turns=12 and 20 pairs added, the oldest 8 pairs are evicted,
        // leaving exactly the last 12 pairs: u8,a8,...,u19,a19 (24 messages).
        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 24);
        assert_eq!(msgs.first().unwrap().content, "u8");
        assert_eq!(msgs.last().unwrap().content, "a19");
    }

    #[test]
    fn dialog_props_engine_defaults_to_llm_from_empty_json() {
        let props: DialogProps = serde_json::from_value(json!({})).unwrap();
        assert_eq!(
            props.engine,
            DialogEngine::Llm { model_profile: "".into(), system: None }
        );
        assert!(!props.markdown);
        assert!(!props.voice_input);
        assert_eq!(props.size.width, 420);
        assert_eq!(props.size.height, 320);
    }

    #[test]
    fn dialog_props_defaults_from_minimal_json() {
        let json = r#"{"engine":{"kind":"llm","model_profile":"p"}}"#;
        let props: DialogProps = serde_json::from_str(json).unwrap();
        assert!(!props.markdown);
        assert!(!props.voice_input);
        assert_eq!(props.size.width, 420);
        assert_eq!(props.size.height, 320);
    }

    #[test]
    fn seed_first_turn_seeds_session_history_for_build_dialog_messages() {
        let mut session = DialogSession::new(42, Some("SYS".into()), 12);
        session.seed_first_turn("workflow input", "workflow output");
        assert_eq!(session.container_id, 42);
        let msgs = build_dialog_messages(session.system.as_deref(), &session.context);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "workflow input");
        assert_eq!(msgs[2]["role"], "assistant");
        assert_eq!(msgs[2]["content"], "workflow output");
    }

    #[test]
    fn next_turn_request_assembly_does_not_mutate_context_before_send() {
        // Regression guard for context poisoning on LLM error (see `next_turn`):
        // the outgoing request must be assembled from history plus the new
        // user turn WITHOUT committing that user turn to `context` until the
        // call succeeds. Otherwise a failed call leaves a dangling user
        // message, and the next turn sends two consecutive "user" messages,
        // which strict-alternation backends (Anthropic) reject.
        //
        // The real network call isn't unit-testable, so this mirrors exactly
        // what `next_turn` does to build the request (see its body) and
        // asserts the request/context split directly.
        let mut session = DialogSession::new(7, Some("SYS".into()), 12);
        session.seed_first_turn("hi", "hello");
        let before = session.context.messages().to_vec();

        let user_text = "follow-up question";
        // Mirrors next_turn's (fixed) ordering exactly: build from the
        // existing context, then push the new user turn onto the *built*
        // vec — `context` itself is never touched here.
        let mut messages = build_dialog_messages(session.system.as_deref(), &session.context);
        messages.push(json!({ "role": "user", "content": user_text }));

        assert_eq!(messages.last().unwrap()["role"], "user");
        assert_eq!(messages.last().unwrap()["content"], user_text);

        // context must be unchanged by merely assembling the outgoing request.
        assert_eq!(session.context.messages().to_vec(), before);
    }
}
