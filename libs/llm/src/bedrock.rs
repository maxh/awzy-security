//! Claude on Amazon Bedrock, in the install's own AWS account (ADR 0048).
//!
//! Requests go through Bedrock Runtime's InvokeModel with Claude's Messages API body. AWS
//! credentials come from the usual chain, which in the cluster is the service's workload
//! identity (EKS Pod Identity), so there's no key to manage. The client enables Claude in their
//! Bedrock account and region; until then requests fail with [`LlmError::NotConfigured`].

use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::{error::ProvideErrorMetadata, primitives::Blob, Client};
use aws_smithy_http_client::tls;
use aws_smithy_types::error::display::DisplayErrorContext;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{Content, LlmError, ModelProvider, ModelRequest, ModelResponse, StopReason, Usage};

/// The Bedrock model ID for Claude Opus 5.5. Some accounts need a regional inference profile
/// instead (e.g. `us.anthropic.claude-opus-5-5`); set AWZY_LLM_MODEL.
pub const DEFAULT_MODEL: &str = "anthropic.claude-opus-5-5";

/// Bedrock's version tag for Claude's Messages API body.
const ANTHROPIC_VERSION: &str = "bedrock-2023-05-31";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BedrockConfig {
    pub model: String,
    /// How hard the model works: low, medium, high, xhigh, or max. None leaves the model's
    /// default.
    pub effort: Option<String>,
}

impl BedrockConfig {
    /// AWZY_LLM_MODEL (default [`DEFAULT_MODEL`]) and AWZY_LLM_EFFORT. The region is AWS_REGION.
    pub fn from_env() -> Self {
        Self {
            model: std::env::var("AWZY_LLM_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
            effort: std::env::var("AWZY_LLM_EFFORT").ok().filter(|e| !e.is_empty()),
        }
    }
}

pub struct Bedrock {
    client: Client,
    config: BedrockConfig,
}

impl Bedrock {
    /// With credentials and region from the environment.
    pub async fn from_env(config: BedrockConfig) -> Self {
        let https = aws_smithy_http_client::Builder::new()
            .tls_provider(tls::Provider::Rustls(tls::rustls_provider::CryptoMode::Ring))
            .build_https();
        let aws = aws_config::defaults(BehaviorVersion::latest()).http_client(https).load().await;
        Self { client: Client::new(&aws), config }
    }
}

impl ModelProvider for Bedrock {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        let body = request_body(request, self.config.effort.as_deref());
        let out = self
            .client
            .invoke_model()
            .model_id(&self.config.model)
            .content_type("application/json")
            .accept("application/json")
            .body(Blob::new(serde_json::to_vec(&body).map_err(|e| LlmError::Failed(e.to_string()))?))
            .send()
            .await
            .map_err(|e| {
                // The whole chain: credential and network failures carry no service error code.
                let detail = DisplayErrorContext(&e).to_string();
                classify(e.into_service_error(), detail)
            })?;
        parse_response(out.body().as_ref())
    }
}

/// Claude's Messages API body, as Bedrock takes it. The system prompt and tools come first and
/// are marked for caching: they're the same on every turn of a conversation.
fn request_body(request: &ModelRequest, effort: Option<&str>) -> Value {
    let mut body = json!({
        "anthropic_version": ANTHROPIC_VERSION,
        "max_tokens": request.max_tokens,
        "messages": request.messages,
    });
    if !request.system.is_empty() {
        body["system"] = json!([{ "type": "text", "text": request.system, "cache_control": { "type": "ephemeral" } }]);
    }
    if !request.tools.is_empty() {
        let mut tools = serde_json::to_value(&request.tools).expect("tool specs serialize");
        if let Some(last) = tools.as_array_mut().and_then(|t| t.last_mut()) {
            last["cache_control"] = json!({ "type": "ephemeral" });
        }
        body["tools"] = tools;
    }
    if let Some(effort) = effort {
        body["output_config"] = json!({ "effort": effort });
    }
    body
}

#[derive(Deserialize)]
struct WireResponse {
    content: Vec<Content>,
    stop_reason: Option<String>,
    #[serde(default)]
    usage: WireUsage,
}

#[derive(Deserialize, Default)]
struct WireUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

fn parse_response(bytes: &[u8]) -> Result<ModelResponse, LlmError> {
    let wire: WireResponse = serde_json::from_slice(bytes).map_err(|e| LlmError::Malformed(e.to_string()))?;
    let u = wire.usage;
    Ok(ModelResponse {
        content: wire.content,
        stop: StopReason::parse(wire.stop_reason.as_deref().unwrap_or("end_turn")),
        usage: Usage {
            input_tokens: u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens,
            output_tokens: u.output_tokens,
        },
    })
}

fn classify<E: ProvideErrorMetadata>(e: E, detail: String) -> LlmError {
    let message = match e.code() {
        Some(code) => format!("{code}: {}", e.message().unwrap_or(&detail)),
        None => detail,
    };
    match e.code() {
        Some(
            "ThrottlingException"
            | "ServiceQuotaExceededException"
            | "ServiceUnavailableException"
            | "ModelNotReadyException",
        ) => LlmError::Unavailable(message),
        Some("AccessDeniedException" | "ResourceNotFoundException" | "UnrecognizedClientException") => {
            LlmError::NotConfigured(message)
        }
        _ => LlmError::Failed(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Message, ToolSpec};

    #[test]
    fn the_request_is_claudes_messages_body_with_the_stable_parts_cached() {
        let request = ModelRequest {
            system: "You are Awzy Bot.".into(),
            messages: vec![Message::user("Why was bob denied?")],
            tools: vec![
                ToolSpec { name: "a".into(), description: "A".into(), input_schema: json!({ "type": "object" }) },
                ToolSpec { name: "b".into(), description: "B".into(), input_schema: json!({ "type": "object" }) },
            ],
            max_tokens: 4096,
        };
        let body = request_body(&request, Some("medium"));
        assert_eq!(body["anthropic_version"], "bedrock-2023-05-31");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(
            body["messages"][0],
            json!({ "role": "user", "content": [{ "type": "text", "text": "Why was bob denied?" }] })
        );
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral", "one breakpoint, after the last tool");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert!(body.get("thinking").is_none(), "the model's default thinking");

        let bare = request_body(&ModelRequest { system: String::new(), tools: vec![], ..request }, None);
        assert!(bare.get("system").is_none() && bare.get("tools").is_none() && bare.get("output_config").is_none());
    }

    #[test]
    fn responses_keep_every_block_and_count_cached_input() {
        let r = parse_response(
            br#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-opus-5-5",
                 "content":[{"type":"thinking","thinking":"","signature":"sig"},
                            {"type":"tool_use","id":"toolu_1","name":"explain_decision","input":{"id":"decision_1"}}],
                 "stop_reason":"tool_use",
                 "usage":{"input_tokens":12,"output_tokens":40,"cache_read_input_tokens":900}}"#,
        )
        .unwrap();
        assert_eq!(r.stop, StopReason::ToolUse);
        assert!(matches!(r.content[0], Content::Other(_)));
        assert!(matches!(r.content[1], Content::ToolUse { .. }));
        assert_eq!(r.usage, Usage { input_tokens: 912, output_tokens: 40 });
        let refused = parse_response(br#"{"content":[],"stop_reason":"refusal"}"#).unwrap();
        assert_eq!(refused.stop, StopReason::Refusal);
    }

    /// Calls Bedrock for real when AWZY_TEST_BEDROCK_MODEL names a model this machine's AWS
    /// credentials can use (e.g. with AWS_PROFILE and AWS_REGION set); skipped otherwise.
    #[tokio::test]
    async fn answers_through_bedrock() {
        let Ok(model) = std::env::var("AWZY_TEST_BEDROCK_MODEL") else {
            eprintln!("skipping: AWZY_TEST_BEDROCK_MODEL is not set");
            return;
        };
        let bedrock = Bedrock::from_env(BedrockConfig { model, effort: Some("low".into()) }).await;
        let request = ModelRequest {
            system: String::new(),
            messages: vec![Message::user("Reply with the single word: ready")],
            tools: vec![],
            max_tokens: 2048,
        };
        let r = bedrock.respond(&request).await.unwrap();
        assert_eq!(r.stop, StopReason::EndTurn);
        assert!(r.text().to_lowercase().contains("ready"), "{}", r.text());
    }
}
