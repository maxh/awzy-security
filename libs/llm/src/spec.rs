//! What a catalog model is, in the native terms of the API it speaks (ADR 0114). The connection
//! says where it is and how awzy signs in; the spec says which model, and the settings that API
//! has. Fields awzy hasn't typed yet go in `extra`, merged into each request as they are.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Connection;

/// A model, by the API it speaks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "api", rename_all = "snake_case")]
pub enum ModelSpec {
    /// Bedrock's Converse: any model on Bedrock.
    Converse(ConverseSpec),
    /// Claude's Messages API, on Vertex AI or Anthropic's own.
    AnthropicMessages(AnthropicSpec),
    /// Gemini's `generateContent`, on Vertex AI.
    Gemini(GeminiSpec),
    /// OpenAI's chat completions: OpenAI, Azure, vLLM, Ollama, and open models on Vertex AI.
    OpenaiChat(OpenaiSpec),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConverseSpec {
    /// A Bedrock model ID or inference profile, e.g. `us.anthropic.claude-opus-5-5`.
    pub model_id: String,
    /// Cache points after the system prompt and the tools, for the models that cache.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cache_points: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// The model's own request fields, as Converse passes them on.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub additional_model_request_fields: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnthropicSpec {
    /// e.g. `claude-opus-5-5`; on Vertex AI, optionally pinned with `@<version>`.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Extended thinking, with this many tokens for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_budget_tokens: Option<u32>,
    /// Mark the system prompt and tools for caching.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prompt_caching: bool,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeminiSpec {
    /// e.g. `gemini-2.5-pro`.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    Minimal,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenaiSpec {
    /// The model as the server names it: `gpt-…`, a deployment name on Azure, `meta/llama-…` on
    /// Vertex AI, whatever vLLM or Ollama serves.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
}

impl ModelSpec {
    /// The model's name in its service, for logs.
    pub fn model(&self) -> &str {
        match self {
            Self::Converse(s) => &s.model_id,
            Self::AnthropicMessages(s) => &s.model,
            Self::Gemini(s) => &s.model,
            Self::OpenaiChat(s) => &s.model,
        }
    }

    /// Whether `connection` offers this API.
    pub fn offered_by(&self, connection: &Connection) -> bool {
        matches!(
            (self, connection),
            (Self::Converse(_), Connection::Bedrock { .. })
                | (Self::Gemini(_), Connection::Vertex { .. })
                | (Self::AnthropicMessages(_), Connection::Vertex { .. } | Connection::Anthropic { .. })
                | (Self::OpenaiChat(_), Connection::Openai { .. } | Connection::Vertex { .. })
        )
    }

    fn api(&self) -> &'static str {
        match self {
            Self::Converse(_) => "converse",
            Self::AnthropicMessages(_) => "anthropic_messages",
            Self::Gemini(_) => "gemini",
            Self::OpenaiChat(_) => "openai_chat",
        }
    }

    /// Why `connection` can't serve it, for a routing that pairs them.
    pub(crate) fn mismatch(&self, connection: &Connection) -> String {
        let kind = match connection {
            Connection::Bedrock { .. } => "bedrock",
            Connection::Vertex { .. } => "vertex",
            Connection::Openai { .. } => "openai",
            Connection::Anthropic { .. } => "anthropic",
        };
        format!("a {kind} connection doesn't offer the {} API", self.api())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn each_api_has_its_own_settings_and_a_typo_is_refused() {
        let gemini: ModelSpec =
            serde_json::from_value(json!({ "api": "gemini", "model": "gemini-2.5-pro", "thinking_level": "low" }))
                .unwrap();
        assert!(matches!(&gemini, ModelSpec::Gemini(s) if s.thinking_level == Some(ThinkingLevel::Low)));
        let converse: ModelSpec = serde_json::from_value(
            json!({ "api": "converse", "model_id": "us.anthropic.claude-opus-5-5", "cache_points": true }),
        )
        .unwrap();
        assert_eq!(converse.model(), "us.anthropic.claude-opus-5-5");

        // Another API's setting, or a misspelled one, isn't quietly ignored.
        assert!(serde_json::from_value::<ModelSpec>(json!({ "api": "gemini", "model": "g", "effort": "low" })).is_err());
        assert!(serde_json::from_value::<ModelSpec>(
            json!({ "api": "openai_chat", "model": "m", "reasoning_efort": "low" })
        )
        .is_err());
        assert!(serde_json::from_value::<ModelSpec>(
            json!({ "api": "anthropic_messages", "model": "c", "effort": "extreme" })
        )
        .is_err());
    }

    #[test]
    fn an_api_runs_only_on_the_services_that_offer_it() {
        let bedrock = Connection::Bedrock { region: "us-east-2".into() };
        let vertex = Connection::Vertex { project: "p".into(), region: "global".into(), token_env: None, api_key_env: None };
        let gemini: ModelSpec = serde_json::from_value(json!({ "api": "gemini", "model": "gemini-2.5-pro" })).unwrap();
        let claude: ModelSpec =
            serde_json::from_value(json!({ "api": "anthropic_messages", "model": "claude-opus-5-5" })).unwrap();
        assert!(gemini.offered_by(&vertex) && !gemini.offered_by(&bedrock));
        assert!(claude.offered_by(&vertex) && !claude.offered_by(&bedrock), "Claude on Bedrock is Converse");
        assert!(gemini.mismatch(&bedrock).contains("bedrock connection doesn't offer the gemini API"));
    }
}
