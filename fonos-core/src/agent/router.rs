//! Processor router — dispatches STT output to either the mode pipeline or the
//! agent processor based on the active user selection.
//!
//! [`ProcessorRouter`] is the single entry point that bridges the voice
//! transcription layer with the two processing paths in Fonos:
//!
//! * **Mode path** — single-pass LLM text transformation using a [`Mode`]
//!   definition (e.g. "polish", "formal", "translate").  When the selected
//!   mode has no prompts (e.g. "raw") the transcribed text is returned as-is.
//! * **Agent path** — multi-step reasoning loop implemented by
//!   [`AgentProcessor`].  The processor owns a persistent
//!   [`ConversationContext`] so that follow-up utterances reference prior
//!   exchanges.
//!
//! # Context lifecycle
//!
//! Conversation context is preserved across selection changes.  Switching from
//! "agent" to a mode (and back) does **not** reset the agent's history —
//! context is only cleared by an explicit call to [`ProcessorRouter::reset_agent`].

use crate::{
    agent::{
        context::ConversationContext,
        fast_path::FastPathMatcher,
        processor::{AgentProcessor, AgentResult, HttpLlmCaller, LlmCaller},
        registry::SkillRegistry,
    },
    llm::{LlmResponse, ServiceConfig},
    modes::built_in_modes,
    Result,
};

// ─── RouterResult ─────────────────────────────────────────────────────────────

/// The outcome of a [`ProcessorRouter::route`] call.
#[derive(Debug)]
pub enum RouterResult {
    /// The selection mapped to a mode that has LLM prompts; the text was
    /// processed through the mode pipeline.
    ModeResult {
        /// The LLM response produced by the mode pipeline.
        response: LlmResponse,
    },
    /// The selection was `"agent"`; the text was processed by the agent
    /// reasoning loop.
    AgentResult {
        /// The result produced by the agent processor.
        response: AgentResult,
    },
    /// The selection mapped to a mode with no prompts (e.g. `"raw"`); the
    /// original transcribed text is returned unchanged.
    RawText {
        /// The unmodified transcription text.
        text: String,
    },
}

// ─── ProcessorRouter ─────────────────────────────────────────────────────────

/// Routes STT output to the appropriate processing backend.
///
/// A single `ProcessorRouter` should be held for the lifetime of an application
/// session.  It owns the [`AgentProcessor`] (and therefore the
/// [`ConversationContext`]) so that agent state persists across selection
/// changes.
///
/// # Generic parameter
///
/// The `C` parameter lets callers (and tests) inject a custom [`LlmCaller`].
/// Production code uses the default [`HttpLlmCaller`]:
///
/// ```rust,ignore
/// let router = ProcessorRouter::new();
/// ```
pub struct ProcessorRouter<C: LlmCaller = HttpLlmCaller> {
    /// Agent processor that owns conversation context and skill registry.
    agent: AgentProcessor<C>,
}

impl ProcessorRouter<HttpLlmCaller> {
    /// Create a new [`ProcessorRouter`] backed by the production HTTP LLM caller.
    ///
    /// The agent processor is initialised with an empty skill registry and an
    /// empty conversation context.  Register skills via the agent processor
    /// before accepting calls if you need skill dispatch.
    pub fn new() -> Self {
        let registry = SkillRegistry::new();
        let context = ConversationContext::new(20);
        let fast_path = FastPathMatcher::new();
        let agent = AgentProcessor::new(
            registry,
            context,
            fast_path,
            "You are a helpful macOS desktop assistant.",
            30,
        );
        Self { agent }
    }
}

impl<C: LlmCaller> ProcessorRouter<C> {
    /// Create a [`ProcessorRouter`] with a custom [`LlmCaller`].
    ///
    /// Primarily intended for unit testing without real network access.
    ///
    /// # Arguments
    ///
    /// * `registry` — pre-populated skill registry.
    /// * `context` — initial conversation context.
    /// * `fast_path` — configured fast-path matcher.
    /// * `system_prompt` — system prompt injected into agent LLM requests.
    /// * `timeout_secs` — skill execution timeout in seconds.
    /// * `llm_caller` — the LLM caller implementation.
    pub fn with_caller(
        registry: SkillRegistry,
        context: ConversationContext,
        fast_path: FastPathMatcher,
        system_prompt: impl Into<String>,
        timeout_secs: u64,
        llm_caller: C,
    ) -> Self {
        let agent = AgentProcessor::with_caller(
            registry,
            context,
            fast_path,
            system_prompt,
            timeout_secs,
            llm_caller,
        );
        Self { agent }
    }

    /// Route a transcribed utterance to the correct processor.
    ///
    /// # Routing logic
    ///
    /// * `"agent"` — delegates to [`AgentProcessor::process`]; returns
    ///   [`RouterResult::AgentResult`].
    /// * A recognised mode ID with LLM prompts — calls
    ///   [`crate::llm::process_text`]; returns [`RouterResult::ModeResult`].
    /// * A recognised mode ID with **no** prompts (e.g. `"raw"`) — returns
    ///   the original text as [`RouterResult::RawText`].
    /// * An unrecognised selection — treated the same as "raw" and returns
    ///   [`RouterResult::RawText`].
    ///
    /// # Errors
    ///
    /// Returns an error if the LLM call (mode or agent) fails.
    pub async fn route(
        &mut self,
        selection: &str,
        text: &str,
        llm_service: &ServiceConfig,
    ) -> Result<RouterResult> {
        if selection == "agent" {
            let result = self.agent.process(text, llm_service).await?;
            return Ok(RouterResult::AgentResult { response: result });
        }

        // Look up the mode in the built-in set.
        let modes = built_in_modes();
        if let Some(mode) = modes.get(selection) {
            // A mode with neither a system prompt nor a user template passes
            // text through unchanged (the "raw" mode).
            if mode.system.is_none() && mode.user_template.is_none() {
                return Ok(RouterResult::RawText {
                    text: text.to_string(),
                });
            }

            // Mode has prompts — run it through the LLM pipeline.
            let response =
                crate::llm::process_text(text, mode, llm_service, None, "").await?;
            return Ok(RouterResult::ModeResult { response });
        }

        // Unknown selection — treat as raw pass-through.
        Ok(RouterResult::RawText {
            text: text.to_string(),
        })
    }

    /// Explicitly reset the agent's conversation context.
    ///
    /// Call this when the user presses "New conversation" or when the session
    /// should restart from a clean slate.  Switching between modes does **not**
    /// implicitly call this method.
    pub fn reset_agent(&mut self) {
        self.agent.reset();
    }

    /// Return a shared reference to the underlying [`AgentProcessor`].
    pub fn agent(&self) -> &AgentProcessor<C> {
        &self.agent
    }

    /// Return a mutable reference to the underlying [`AgentProcessor`].
    pub fn agent_mut(&mut self) -> &mut AgentProcessor<C> {
        &mut self.agent
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_processor_router {
    use std::pin::Pin;

    use serde_json::{json, Value};

    use super::*;

    // ── Mock LLM caller ──────────────────────────────────────────────────────

    /// A mock [`LlmCaller`] that returns a configurable plain-text response.
    struct MockLlmCaller {
        /// The response text to embed in a synthetic OpenAI-style response.
        response_text: String,
    }

    impl MockLlmCaller {
        fn new(text: impl Into<String>) -> Self {
            Self {
                response_text: text.into(),
            }
        }
    }

    impl LlmCaller for MockLlmCaller {
        fn call<'a>(
            &'a self,
            _service: &'a ServiceConfig,
            _messages: Vec<Value>,
            _tools: Vec<Value>,
        ) -> Pin<Box<dyn std::future::Future<Output = crate::Result<Value>> + Send + 'a>>
        {
            let text = self.response_text.clone();
            Box::pin(async move {
                Ok(json!({
                    "choices": [{
                        "message": {
                            "content": text,
                            "tool_calls": null
                        }
                    }]
                }))
            })
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    /// Build a [`ServiceConfig`] suitable for tests (no real calls are made).
    fn test_service() -> ServiceConfig {
        ServiceConfig {
            provider: "openai".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-4o".to_string(),
            base_url: String::new(),
        }
    }

    /// Build a [`ProcessorRouter`] backed by the mock LLM caller.
    fn mock_router(response: &str) -> ProcessorRouter<MockLlmCaller> {
        ProcessorRouter::with_caller(
            SkillRegistry::new(),
            ConversationContext::new(20),
            FastPathMatcher::new(),
            "Test system prompt",
            30,
            MockLlmCaller::new(response),
        )
    }

    // ── Routing tests ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn route_raw_returns_raw_text() {
        let mut router = mock_router("ignored");
        let svc = test_service();

        let result = router.route("raw", "hello world", &svc).await.unwrap();

        match result {
            RouterResult::RawText { text } => assert_eq!(text, "hello world"),
            other => panic!("expected RawText, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_polish_returns_mode_result() {
        // For "polish" the mock LLM would be called, but we're testing routing
        // logic. Since process_text would make a real HTTP call, we verify the
        // routing decision by checking that "polish" maps to a mode with prompts.
        // We test the routing decision without actually calling process_text by
        // inspecting the built-in modes directly.
        let modes = built_in_modes();
        let polish = modes.get("polish").expect("polish mode should exist");
        assert!(
            polish.system.is_some() || polish.user_template.is_some(),
            "polish mode should have prompts — would route to ModeResult"
        );
    }

    #[tokio::test]
    async fn route_agent_returns_agent_result() {
        let mut router = mock_router("The agent response");
        let svc = test_service();

        let result = router.route("agent", "what is the weather?", &svc).await.unwrap();

        match result {
            RouterResult::AgentResult { response } => {
                assert_eq!(response.response_text, "The agent response");
            }
            other => panic!("expected AgentResult, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn route_unknown_selection_returns_raw_text() {
        let mut router = mock_router("ignored");
        let svc = test_service();

        let result = router
            .route("nonexistent_mode", "some text", &svc)
            .await
            .unwrap();

        match result {
            RouterResult::RawText { text } => assert_eq!(text, "some text"),
            other => panic!("expected RawText, got {other:?}"),
        }
    }

    // ── Context persistence tests ────────────────────────────────────────────

    #[tokio::test]
    async fn agent_context_persists_across_multiple_agent_calls() {
        let mut router = mock_router("response");
        let svc = test_service();

        // First agent call.
        router.route("agent", "turn one", &svc).await.unwrap();
        // Second agent call — context should include the first turn.
        router.route("agent", "turn two", &svc).await.unwrap();

        let messages = router.agent().context().messages();
        // Should have at least 4 messages: user1, asst1, user2, asst2.
        assert!(
            messages.len() >= 4,
            "expected at least 4 messages, got {}",
            messages.len()
        );
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "turn one");
    }

    #[tokio::test]
    async fn switching_to_mode_does_not_reset_agent_context() {
        let mut router = mock_router("agent says hi");
        let svc = test_service();

        // Establish agent context with two turns.
        router.route("agent", "hello", &svc).await.unwrap();

        let msg_count_before = router.agent().context().messages().len();

        // Switch to raw mode (no real LLM call).
        router.route("raw", "some text", &svc).await.unwrap();

        // Agent context should be unchanged.
        let msg_count_after = router.agent().context().messages().len();
        assert_eq!(
            msg_count_before, msg_count_after,
            "switching to a mode should NOT reset agent context"
        );
    }

    #[tokio::test]
    async fn switching_back_to_agent_preserves_context() {
        let mut router = mock_router("agent says hi");
        let svc = test_service();

        // Build up some agent history.
        router.route("agent", "first message", &svc).await.unwrap();

        // Temporarily switch to raw and back.
        router.route("raw", "interlude", &svc).await.unwrap();
        router.route("agent", "second message", &svc).await.unwrap();

        let messages = router.agent().context().messages();
        // Should contain first message exchange + second message exchange.
        assert!(
            messages.len() >= 4,
            "expected at least 4 messages after switch-back, got {}",
            messages.len()
        );
        // First message is still in history.
        assert_eq!(messages[0].content, "first message");
    }

    #[tokio::test]
    async fn reset_agent_clears_context() {
        let mut router = mock_router("hi");
        let svc = test_service();

        router.route("agent", "hello", &svc).await.unwrap();
        assert!(
            !router.agent().context().messages().is_empty(),
            "context should have messages before reset"
        );

        router.reset_agent();

        assert!(
            router.agent().context().messages().is_empty(),
            "context should be empty after reset_agent"
        );
    }

    // ── Built-in mode routing logic ──────────────────────────────────────────

    #[test]
    fn raw_mode_has_no_prompts() {
        let modes = built_in_modes();
        let raw = modes.get("raw").expect("raw mode should exist");
        assert!(
            raw.system.is_none() && raw.user_template.is_none(),
            "raw mode should have no prompts — routes to RawText"
        );
    }

    #[test]
    fn polish_mode_has_prompts() {
        let modes = built_in_modes();
        let polish = modes.get("polish").expect("polish mode should exist");
        assert!(
            polish.system.is_some() || polish.user_template.is_some(),
            "polish mode should have at least one prompt — routes to ModeResult"
        );
    }

    #[test]
    fn formal_mode_has_prompts() {
        let modes = built_in_modes();
        let formal = modes.get("formal").expect("formal mode should exist");
        assert!(
            formal.system.is_some() || formal.user_template.is_some(),
            "formal mode should have at least one prompt — routes to ModeResult"
        );
    }

    #[test]
    fn all_built_in_modes_classified_correctly() {
        // "raw" → RawText, everything else → ModeResult (when LLM available).
        let modes = built_in_modes();
        for (id, mode) in &modes {
            let has_prompts = mode.system.is_some() || mode.user_template.is_some();
            if id == "raw" {
                assert!(
                    !has_prompts,
                    "raw mode should have no prompts"
                );
            } else {
                assert!(
                    has_prompts,
                    "mode '{id}' should have at least one prompt"
                );
            }
        }
    }
}
