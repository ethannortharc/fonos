//! Conversation context for the agent — maintains message history across turns.
//!
//! [`ConversationContext`] stores the rolling window of messages exchanged between
//! the user and the agent (including tool calls and results).  It enforces a
//! configurable maximum number of turns and formats messages into the OpenAI
//! chat-completion wire format via [`ConversationContext::to_messages`].

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A single tool call requested by the assistant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Unique identifier assigned by the LLM for this tool call.
    pub id: String,
    /// Name of the function (skill) to invoke.
    pub function_name: String,
    /// JSON-encoded arguments string as returned by the LLM.
    pub arguments: String,
}

/// A single message in the conversation.
///
/// Roles follow the OpenAI convention: `"user"`, `"assistant"`, or `"tool"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    /// The role of the message author.
    pub role: String,
    /// Text content of the message.
    pub content: String,
    /// Tool calls requested by the assistant in this message, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// The tool call ID this message is responding to (role == `"tool"` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Rolling conversation history for an agent session.
///
/// Maintains up to `max_turns` user/assistant exchange pairs.  When the limit
/// is exceeded the oldest pair is dropped (a leading system message, if present,
/// is always preserved).
#[derive(Debug, Clone)]
pub struct ConversationContext {
    /// The accumulated message history.
    messages: Vec<Message>,
    /// Maximum number of user/assistant turn *pairs* to retain.
    max_turns: usize,
}

impl ConversationContext {
    /// Create a new, empty conversation context.
    ///
    /// # Arguments
    ///
    /// * `max_turns` — maximum number of user/assistant exchange pairs before
    ///   the oldest pair is evicted.  A value of `0` means no limit.
    pub fn new(max_turns: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_turns,
        }
    }

    /// Append a user message to the history.
    ///
    /// After appending, the truncation policy is applied if needed.
    pub fn add_user_message(&mut self, text: &str) {
        self.messages.push(Message {
            role: "user".to_string(),
            content: text.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
        self.truncate_if_needed();
    }

    /// Append a plain assistant (agent) response to the history.
    ///
    /// After appending, the truncation policy is applied if needed.
    pub fn add_agent_response(&mut self, text: &str) {
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: text.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });
        self.truncate_if_needed();
    }

    /// Append an assistant message that contains a tool call request.
    ///
    /// The content of this message is left empty; the LLM-issued [`ToolCall`]
    /// is stored in the `tool_calls` field.
    pub fn add_tool_call(&mut self, call: ToolCall) {
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![call]),
            tool_call_id: None,
        });
        self.truncate_if_needed();
    }

    /// Append the result returned by a skill execution.
    ///
    /// # Arguments
    ///
    /// * `call_id` — the `id` from the [`ToolCall`] that was executed.
    /// * `output` — the text output produced by the skill.
    pub fn add_tool_result(&mut self, call_id: &str, output: &str) {
        self.messages.push(Message {
            role: "tool".to_string(),
            content: output.to_string(),
            tool_calls: None,
            tool_call_id: Some(call_id.to_string()),
        });
        self.truncate_if_needed();
    }

    /// Return a read-only slice of the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Format the message history as a `Vec<serde_json::Value>` ready to be
    /// embedded in an OpenAI chat-completion request.
    ///
    /// Each message is converted to the appropriate JSON shape:
    ///
    /// - **user** → `{"role": "user", "content": "..."}`
    /// - **assistant (plain)** → `{"role": "assistant", "content": "..."}`
    /// - **assistant (tool call)** → `{"role": "assistant", "content": null,
    ///   "tool_calls": [{"id": ..., "type": "function", "function": {"name": ...,
    ///   "arguments": ...}}]}`
    /// - **tool** → `{"role": "tool", "tool_call_id": "...", "content": "..."}`
    pub fn to_messages(&self) -> Vec<Value> {
        self.messages
            .iter()
            .map(|m| match m.role.as_str() {
                "assistant" if m.tool_calls.is_some() => {
                    let calls: Vec<Value> = m
                        .tool_calls
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.function_name,
                                    "arguments": tc.arguments,
                                }
                            })
                        })
                        .collect();
                    json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": calls,
                    })
                }
                "tool" => {
                    json!({
                        "role": "tool",
                        "tool_call_id": m.tool_call_id.as_deref().unwrap_or(""),
                        "content": m.content,
                    })
                }
                _ => {
                    json!({
                        "role": m.role,
                        "content": m.content,
                    })
                }
            })
            .collect()
    }

    /// Clear all messages from the history.
    pub fn reset(&mut self) {
        self.messages.clear();
    }

    // ── Private helpers ─────────────────────────────────────────────────────

    /// Enforce the `max_turns` limit by dropping the oldest user/assistant pair.
    ///
    /// A message whose role is `"system"` at index 0 is treated as the system
    /// prompt and is never dropped.  The limit is enforced such that the number
    /// of non-system messages never exceeds `max_turns * 2`.
    fn truncate_if_needed(&mut self) {
        if self.max_turns == 0 {
            return;
        }

        loop {
            // Determine the start index for non-system messages.
            let start = if self.messages.first().map(|m| m.role.as_str()) == Some("system") {
                1
            } else {
                0
            };

            let non_system_count = self.messages.len() - start;

            // Allow at most max_turns * 2 non-system messages.
            if non_system_count <= self.max_turns * 2 {
                break;
            }

            // Drop the first two messages after the optional system prompt.
            self.messages.remove(start);
            if start < self.messages.len() {
                self.messages.remove(start);
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test_conversation_context {
    use super::*;

    #[test]
    fn new_context_starts_empty() {
        let ctx = ConversationContext::new(10);
        assert!(ctx.messages().is_empty());
        assert!(ctx.to_messages().is_empty());
    }

    #[test]
    fn add_user_message_builds_history() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_user_message("Hello");
        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
    }

    #[test]
    fn add_agent_response_builds_history() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_user_message("Hello");
        ctx.add_agent_response("Hi there!");
        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "Hi there!");
    }

    #[test]
    fn to_messages_formats_user_and_assistant() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_user_message("What time is it?");
        ctx.add_agent_response("It is noon.");

        let msgs = ctx.to_messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "What time is it?");
        assert_eq!(msgs[1]["role"], "assistant");
        assert_eq!(msgs[1]["content"], "It is noon.");
    }

    #[test]
    fn add_tool_call_creates_assistant_message_with_tool_calls() {
        let mut ctx = ConversationContext::new(10);
        let call = ToolCall {
            id: "call_abc".to_string(),
            function_name: "shell".to_string(),
            arguments: r#"{"command":"whoami"}"#.to_string(),
        };
        ctx.add_tool_call(call.clone());

        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "assistant");
        let tc = msgs[0].tool_calls.as_ref().unwrap();
        assert_eq!(tc.len(), 1);
        assert_eq!(tc[0].id, "call_abc");
        assert_eq!(tc[0].function_name, "shell");
    }

    #[test]
    fn add_tool_result_creates_tool_message() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_tool_result("call_abc", "ethan");

        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "tool");
        assert_eq!(msgs[0].tool_call_id.as_deref(), Some("call_abc"));
        assert_eq!(msgs[0].content, "ethan");
    }

    #[test]
    fn to_messages_formats_tool_call_correctly() {
        let mut ctx = ConversationContext::new(10);
        let call = ToolCall {
            id: "call_1".to_string(),
            function_name: "shell".to_string(),
            arguments: r#"{"command":"date"}"#.to_string(),
        };
        ctx.add_tool_call(call);

        let msgs = ctx.to_messages();
        assert_eq!(msgs[0]["role"], "assistant");
        // content should be null for tool-call messages
        assert!(msgs[0]["content"].is_null());
        let tool_calls = &msgs[0]["tool_calls"];
        assert!(tool_calls.is_array());
        let first = &tool_calls[0];
        assert_eq!(first["id"], "call_1");
        assert_eq!(first["type"], "function");
        assert_eq!(first["function"]["name"], "shell");
        assert_eq!(first["function"]["arguments"], r#"{"command":"date"}"#);
    }

    #[test]
    fn to_messages_formats_tool_result_correctly() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_tool_result("call_1", "Tue Mar 24 12:00:00 UTC 2026");

        let msgs = ctx.to_messages();
        assert_eq!(msgs[0]["role"], "tool");
        assert_eq!(msgs[0]["tool_call_id"], "call_1");
        assert_eq!(msgs[0]["content"], "Tue Mar 24 12:00:00 UTC 2026");
    }

    #[test]
    fn follow_up_references_include_prior_exchange() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_user_message("What is the capital of France?");
        ctx.add_agent_response("Paris.");
        ctx.add_user_message("And what is its population?");
        ctx.add_agent_response("About 2.1 million in the city proper.");

        let msgs = ctx.to_messages();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[0]["content"], "What is the capital of France?");
        assert_eq!(msgs[1]["content"], "Paris.");
        assert_eq!(msgs[2]["content"], "And what is its population?");
        assert_eq!(msgs[3]["content"], "About 2.1 million in the city proper.");
    }

    #[test]
    fn reset_clears_all_history() {
        let mut ctx = ConversationContext::new(10);
        ctx.add_user_message("Hello");
        ctx.add_agent_response("Hi");
        ctx.reset();
        assert!(ctx.messages().is_empty());
        assert!(ctx.to_messages().is_empty());
    }

    #[test]
    fn truncation_drops_oldest_pair_when_max_exceeded() {
        // max_turns = 2 means we keep at most 2 user/assistant pairs (4 msgs)
        let mut ctx = ConversationContext::new(2);

        ctx.add_user_message("Turn 1 user");
        ctx.add_agent_response("Turn 1 assistant");
        ctx.add_user_message("Turn 2 user");
        ctx.add_agent_response("Turn 2 assistant");
        // At 2 pairs, we're exactly at the limit.
        assert_eq!(ctx.messages().len(), 4);

        // Adding a 3rd user message triggers truncation: oldest pair drops.
        ctx.add_user_message("Turn 3 user");
        // After truncation we have: turn2 user, turn2 assistant, turn3 user
        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "Turn 2 user");
        assert_eq!(msgs[1].content, "Turn 2 assistant");
        assert_eq!(msgs[2].content, "Turn 3 user");
    }

    #[test]
    fn truncation_preserves_leading_system_message() {
        let mut ctx = ConversationContext::new(1);
        // Manually add a system message as the first entry.
        ctx.messages.push(Message {
            role: "system".to_string(),
            content: "You are a helpful assistant.".to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        ctx.add_user_message("Turn 1 user");
        ctx.add_agent_response("Turn 1 assistant");
        // At max_turns=1 we have [system, user1, asst1] = 1 non-system pair -> still ok.
        assert_eq!(ctx.messages().len(), 3);

        // Adding turn 2 user pushes to 3 non-system messages (> max_turns*2=2).
        // The oldest non-system pair (user1, asst1) is dropped.
        ctx.add_user_message("Turn 2 user");
        // After truncation: [system, user2] = 2 messages.
        let msgs = ctx.messages();
        assert_eq!(msgs.len(), 2);
        // System message is always preserved at index 0.
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[0].content, "You are a helpful assistant.");
        // The surviving message is turn 2's user message.
        assert_eq!(msgs[1].content, "Turn 2 user");
    }

    #[test]
    fn zero_max_turns_means_no_limit() {
        let mut ctx = ConversationContext::new(0);
        for i in 0..50 {
            ctx.add_user_message(&format!("msg {i}"));
            ctx.add_agent_response(&format!("resp {i}"));
        }
        assert_eq!(ctx.messages().len(), 100);
    }

    #[test]
    fn tool_call_round_trip_serialises() {
        let tc = ToolCall {
            id: "c1".to_string(),
            function_name: "shell".to_string(),
            arguments: "{}".to_string(),
        };
        let json = serde_json::to_string(&tc).unwrap();
        let tc2: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(tc, tc2);
    }

    #[test]
    fn message_serialises_and_deserialises() {
        let msg = Message {
            role: "user".to_string(),
            content: "hello".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let msg2: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, msg2);
    }
}
