//! Model access for awzy's AI features (ADR 0048, 0114): Awzy Bot's policy drafting and
//! explanations, the reasoner's investigations, and later classification and justification checks.
//!
//! Callers speak in neutral types ([`ModelRequest`], [`Message`], [`Content`]) and get a model
//! for each conversation from a [`ModelRouter`], which picks one from the install's catalog by
//! the feature's route: any model, on any service the install can reach. Each wire format is an
//! adapter behind those types: Bedrock's Converse API ([`converse`]), Claude's Messages API
//! ([`anthropic`]), Gemini's `generateContent` ([`gemini`]), and OpenAI's chat completions
//! ([`openai`]), which covers OpenAI, Azure, self-hosted, and open models.
//!
//! Nothing on the awzy-pdp hot path may call this, and model output is only ever a proposal:
//! the callers' tools read, explain, and draft, and a person acts.

pub mod anthropic;
mod connections;
pub mod converse;
pub mod fake;
pub mod gemini;
pub mod openai;
mod router;
mod spec;
mod tools;

use std::{future::Future, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub use connections::{Connection, Connections};
pub use router::{Conversation, ModelEntry, ModelRouter, Route, RouteTarget, Routing};
pub use spec::{
    AnthropicSpec, ConverseSpec, Effort, GeminiSpec, ModelSpec, OpenaiSpec, ReasoningEffort, ThinkingLevel,
};
pub use tools::{run_with_tools, Tool, ToolOutcome, ToolRun};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// One block of a message. Every model's own blocks are translated to and from these.
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
    /// What only the model that wrote it understands, in that model's wire `format`: Claude's
    /// thinking, Gemini's thought signatures. Sent back unchanged to a model of the same format,
    /// which needs it to continue, and left out for any other.
    Opaque {
        format: String,
        data: Value,
    },
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
    /// Instructions that frame the conversation. Kept the same across turns so the service can
    /// cache it.
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
    /// The catalog entry that answered (its model ID, outside a router), for what a run records.
    pub model: String,
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
    /// Model access isn't set up: no permission, the model isn't enabled, the ID is wrong, or no
    /// route names a model for the feature.
    #[error("the model isn't available to this install: {0}")]
    NotConfigured(String),
    #[error("the model service failed: {0}")]
    Failed(String),
    #[error("the model's answer couldn't be read: {0}")]
    Malformed(String),
}

/// A model to talk to. Implementations: a router's [`Conversation`], each wire format's model,
/// and [`fake::FakeModel`] for tests.
pub trait ModelProvider: Send + Sync {
    fn respond(&self, request: &ModelRequest) -> impl Future<Output = Result<ModelResponse, LlmError>> + Send;
}

impl<P: ModelProvider> ModelProvider for Arc<P> {
    fn respond(&self, request: &ModelRequest) -> impl Future<Output = Result<ModelResponse, LlmError>> + Send {
        self.as_ref().respond(request)
    }
}

/// Where a feature gets a model for each conversation, so every turn of one goes to the same
/// model: [`ModelRouter`] in an install, a scripted model in tests.
pub trait Models: Send + Sync {
    type Conversation: ModelProvider;
    /// A model for one conversation of `feature` (`awzy_bot`, `investigations`).
    fn conversation(&self, feature: &str) -> Result<Self::Conversation, LlmError>;
}

impl Models for Arc<fake::FakeModel> {
    type Conversation = Arc<fake::FakeModel>;
    fn conversation(&self, _feature: &str) -> Result<Self::Conversation, LlmError> {
        Ok(self.clone())
    }
}

/// `params` merged into `body`, objects key by key: a spec's `extra`, the settings awzy hasn't
/// typed, reaching the model as they are.
pub(crate) fn merge_params(body: &mut Value, params: &Map<String, Value>) {
    let Value::Object(target) = body else { return };
    for (k, v) in params {
        match (target.get_mut(k), v) {
            (Some(existing @ Value::Object(_)), Value::Object(inner)) => merge_params(existing, inner),
            _ => {
                target.insert(k.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_round_trips_with_what_only_its_model_understands() {
        let blocks = json!([
            { "type": "opaque", "format": "anthropic", "data": { "type": "thinking", "thinking": "", "signature": "abc" } },
            { "type": "text", "text": "Bob was denied because…" },
            { "type": "tool_use", "id": "toolu_1", "name": "explain_decision", "input": { "id": "decision_x" } },
        ]);
        let parsed: Vec<Content> = serde_json::from_value(blocks.clone()).unwrap();
        assert!(matches!(parsed[0], Content::Opaque { .. }));
        assert!(matches!(parsed[1], Content::Text { .. }));
        assert!(matches!(parsed[2], Content::ToolUse { .. }));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), blocks);
    }

    #[test]
    fn params_merge_into_the_body_object_by_object() {
        let mut body = json!({ "max_tokens": 10, "generationConfig": { "maxOutputTokens": 10 } });
        let params = json!({ "temperature": 0.2, "generationConfig": { "thinkingConfig": { "thinkingLevel": "low" } } });
        merge_params(&mut body, params.as_object().unwrap());
        assert_eq!(
            body,
            json!({ "max_tokens": 10, "temperature": 0.2,
                    "generationConfig": { "maxOutputTokens": 10, "thinkingConfig": { "thinkingLevel": "low" } } })
        );
    }
}
