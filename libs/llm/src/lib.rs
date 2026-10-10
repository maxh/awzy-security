//! Model access for awzy's AI features, Awzy Bot first (ADR 0048): policy drafting, explaining
//! decisions, and later classification, justification checks, and investigations.
//!
//! Services build a [`ModelRequest`] and call a [`ModelProvider`]. The provider is the
//! install's own cloud model service, so prompts never leave the client's account: Amazon
//! Bedrock today ([`bedrock`]); Vertex AI and Azure follow behind the same trait.
//!
//! Nothing on the awzy-pdp hot path may call this, and model output is only ever a proposal:
//! the callers' tools read, explain, and draft, and a person acts.

pub mod bedrock;
pub mod fake;
mod tools;

use std::future::Future;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use tools::{run_with_tools, Tool, ToolOutcome, ToolRun};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// One block of a message, in the shape of Claude's Messages API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },
    /// Any other block the model returns (its thinking, for one). It's passed back unchanged
    /// when the conversation continues, which the model requires.
    #[serde(untagged)]
    Other(Value),
}

impl Content {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![Content::text(text)] }
    }
}

/// A tool the model may call: a name, what it's for, and a JSON Schema for its input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequest {
    /// Instructions that frame the conversation. Kept the same across turns so the provider
    /// can cache it.
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    /// The model declined. The caller says so to the person rather than retrying.
    Refusal,
    Other(String),
}

impl StopReason {
    fn parse(s: &str) -> Self {
        match s {
            "end_turn" | "stop_sequence" => Self::EndTurn,
            "tool_use" => Self::ToolUse,
            "max_tokens" => Self::MaxTokens,
            "refusal" => Self::Refusal,
            other => Self::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, other: Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelResponse {
    pub content: Vec<Content>,
    pub stop: StopReason,
    pub usage: Usage,
}

impl ModelResponse {
    /// The text blocks, joined.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// The model service turned the request away for now (throttling, quota); try later.
    #[error("the model service is busy or over quota: {0}")]
    Unavailable(String),
    /// Model access isn't set up: no permission, the model isn't enabled, or the ID is wrong.
    #[error("the model isn't available to this install: {0}")]
    NotConfigured(String),
    #[error("the model service failed: {0}")]
    Failed(String),
    #[error("the model's answer couldn't be read: {0}")]
    Malformed(String),
}

/// A model service. Implementations: [`bedrock::Bedrock`], and [`fake::FakeModel`] for tests.
pub trait ModelProvider: Send + Sync {
    fn respond(&self, request: &ModelRequest) -> impl Future<Output = Result<ModelResponse, LlmError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_round_trips_including_blocks_awzy_doesnt_know() {
        let blocks = json!([
            { "type": "thinking", "thinking": "", "signature": "abc" },
            { "type": "text", "text": "Bob was denied because…" },
            { "type": "tool_use", "id": "toolu_1", "name": "explain_decision", "input": { "id": "decision_x" } },
        ]);
        let parsed: Vec<Content> = serde_json::from_value(blocks.clone()).unwrap();
        assert!(matches!(parsed[0], Content::Other(_)));
        assert!(matches!(parsed[1], Content::Text { .. }));
        assert!(matches!(parsed[2], Content::ToolUse { .. }));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), blocks, "passed back byte for byte");
    }
}
