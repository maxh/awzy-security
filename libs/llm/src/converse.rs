//! Amazon Bedrock's Converse API: every model on Bedrock (Claude, Llama, Mistral, Nova, and the
//! rest) through one model-neutral request, in the install's own AWS account. Credentials come
//! from the usual chain, which in the cluster is the service's workload identity.

use std::collections::HashMap;

use aws_config::BehaviorVersion;
use aws_sdk_bedrockruntime::{
    error::ProvideErrorMetadata,
    types::{
        self as bedrock, CachePointBlock, CachePointType, ContentBlock, ConversationRole, InferenceConfiguration,
        ReasoningContentBlock, ReasoningTextBlock, SystemContentBlock, ToolConfiguration, ToolInputSchema,
        ToolResultBlock, ToolResultContentBlock, ToolResultStatus, ToolSpecification, ToolUseBlock,
    },
    Client,
};
use aws_smithy_http_client::tls;
use aws_smithy_types::{error::display::DisplayErrorContext, Document, Number};
use serde_json::{json, Value};

use crate::{Content, ConverseSpec, LlmError, Message, ModelRequest, ModelResponse, Role, StopReason, Usage};

/// This format's name on [`Content::Opaque`] blocks.
pub const FORMAT: &str = "converse";

/// A Bedrock Runtime client in `region`.
pub async fn client(region: &str) -> Client {
    let https = aws_smithy_http_client::Builder::new()
        .tls_provider(tls::Provider::Rustls(tls::rustls_provider::CryptoMode::Ring))
        .build_https();
    let aws = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_string()))
        .http_client(https)
        .load()
        .await;
    Client::new(&aws)
}

/// Asks the spec's model. With cache points, the system prompt and tools are marked for the
/// models that cache.
pub async fn respond(client: &Client, spec: &ConverseSpec, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
    let (model, cache) = (spec.model_id.as_str(), spec.cache_points);
    let built = |e: aws_smithy_types::error::operation::BuildError| LlmError::Failed(e.to_string());
    let cache_point = || CachePointBlock::builder().r#type(CachePointType::Default).build().map_err(built);
    let mut inference = InferenceConfiguration::builder().max_tokens(request.max_tokens as i32);
    if let Some(t) = spec.temperature {
        inference = inference.temperature(t);
    }
    let mut call = client.converse().model_id(model).inference_config(inference.build());
    if !request.system.is_empty() {
        call = call.system(SystemContentBlock::Text(request.system.clone()));
        if cache {
            call = call.system(SystemContentBlock::CachePoint(cache_point()?));
        }
    }
    for m in &request.messages {
        call = call.messages(message(m).map_err(built)?);
    }
    if !request.tools.is_empty() {
        let mut tools = ToolConfiguration::builder();
        for t in &request.tools {
            let spec = ToolSpecification::builder()
                .name(&t.name)
                .description(&t.description)
                .input_schema(ToolInputSchema::Json(document(&t.input_schema)))
                .build()
                .map_err(built)?;
            tools = tools.tools(bedrock::Tool::ToolSpec(spec));
        }
        if cache {
            tools = tools.tools(bedrock::Tool::CachePoint(cache_point()?));
        }
        call = call.tool_config(tools.build().map_err(built)?);
    }
    if !spec.additional_model_request_fields.is_empty() {
        call = call.additional_model_request_fields(document(&Value::Object(spec.additional_model_request_fields.clone())));
    }
    let out = call.send().await.map_err(|e| {
        // The whole chain: credential and network failures carry no service error code.
        let detail = DisplayErrorContext(&e).to_string();
        classify(e.into_service_error(), detail)
    })?;

    let mut content = Vec::new();
    if let Some(Ok(m)) = out.output().map(|o| o.as_message()) {
        for block in m.content() {
            match block {
                ContentBlock::Text(text) => content.push(Content::Text { text: text.clone() }),
                ContentBlock::ToolUse(u) => content.push(Content::ToolUse {
                    id: u.tool_use_id().to_string(),
                    name: u.name().to_string(),
                    input: json_value(u.input()),
                }),
                ContentBlock::ReasoningContent(r) => {
                    // Redacted reasoning can't travel as JSON; a model that sends it continues without it.
                    if let Ok(t) = r.as_reasoning_text() {
                        content.push(Content::Opaque {
                            format: FORMAT.into(),
                            data: json!({ "reasoningText": { "text": t.text(), "signature": t.signature() } }),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    let stop = match out.stop_reason() {
        bedrock::StopReason::EndTurn | bedrock::StopReason::StopSequence => StopReason::EndTurn,
        bedrock::StopReason::ToolUse => StopReason::ToolUse,
        bedrock::StopReason::MaxTokens => StopReason::MaxTokens,
        bedrock::StopReason::ContentFiltered | bedrock::StopReason::GuardrailIntervened => StopReason::Refusal,
        other => StopReason::Other(other.as_str().to_string()),
    };
    let usage = out.usage().map_or(Usage::default(), |u| Usage {
        input_tokens: (u.input_tokens()
            + u.cache_read_input_tokens().unwrap_or(0)
            + u.cache_write_input_tokens().unwrap_or(0))
        .max(0) as u64,
        output_tokens: u.output_tokens().max(0) as u64,
    });
    Ok(ModelResponse { content, stop, usage, model: model.to_string() })
}

fn message(m: &Message) -> Result<bedrock::Message, aws_smithy_types::error::operation::BuildError> {
    let role = match m.role {
        Role::User => ConversationRole::User,
        Role::Assistant => ConversationRole::Assistant,
    };
    let mut out = bedrock::Message::builder().role(role);
    for c in &m.content {
        let block = match c {
            Content::Text { text } => ContentBlock::Text(text.clone()),
            Content::ToolUse { id, name, input } => {
                ContentBlock::ToolUse(ToolUseBlock::builder().tool_use_id(id).name(name).input(document(input)).build()?)
            }
            Content::ToolResult { tool_use_id, content, is_error } => ContentBlock::ToolResult(
                ToolResultBlock::builder()
                    .tool_use_id(tool_use_id)
                    .content(ToolResultContentBlock::Text(content.clone()))
                    .status(if *is_error { ToolResultStatus::Error } else { ToolResultStatus::Success })
                    .build()?,
            ),
            Content::Opaque { format, data } if format == FORMAT => {
                let Some(text) = data.pointer("/reasoningText/text").and_then(Value::as_str) else { continue };
                let mut reasoning = ReasoningTextBlock::builder().text(text);
                if let Some(sig) = data.pointer("/reasoningText/signature").and_then(Value::as_str) {
                    reasoning = reasoning.signature(sig);
                }
                ContentBlock::ReasoningContent(ReasoningContentBlock::ReasoningText(reasoning.build()?))
            }
            Content::Opaque { .. } => continue,
        };
        out = out.content(block);
    }
    out.build()
}

/// JSON as the SDK's [`Document`].
fn document(v: &Value) -> Document {
    match v {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(*b),
        Value::Number(n) => Document::Number(match (n.as_u64(), n.as_i64()) {
            (Some(u), _) => Number::PosInt(u),
            (None, Some(i)) => Number::NegInt(i),
            _ => Number::Float(n.as_f64().unwrap_or(0.0)),
        }),
        Value::String(s) => Document::String(s.clone()),
        Value::Array(items) => Document::Array(items.iter().map(document).collect()),
        Value::Object(o) => Document::Object(o.iter().map(|(k, v)| (k.clone(), document(v))).collect::<HashMap<_, _>>()),
    }
}

/// The SDK's [`Document`] as JSON.
fn json_value(d: &Document) -> Value {
    match d {
        Document::Null => Value::Null,
        Document::Bool(b) => Value::Bool(*b),
        Document::Number(Number::PosInt(u)) => json!(u),
        Document::Number(Number::NegInt(i)) => json!(i),
        Document::Number(Number::Float(f)) => json!(f),
        Document::String(s) => Value::String(s.clone()),
        Document::Array(items) => Value::Array(items.iter().map(json_value).collect()),
        Document::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), json_value(v))).collect()),
    }
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
    use serde_json::Map;

    #[test]
    fn json_and_documents_convert_both_ways() {
        let v = json!({ "type": "object", "n": -3, "big": 7, "f": 0.5, "list": [true, null, "x"] });
        assert_eq!(json_value(&document(&v)), v);
    }

    #[test]
    fn a_conversation_becomes_converse_messages_with_only_its_own_reasoning() {
        let m = Message {
            role: Role::Assistant,
            content: vec![
                Content::Opaque { format: FORMAT.into(), data: json!({ "reasoningText": { "text": "hm", "signature": "s" } }) },
                Content::Opaque { format: "gemini".into(), data: json!({ "thoughtSignature": "g" }) },
                Content::Text { text: "Looking.".into() },
                Content::ToolUse { id: "t1".into(), name: "explain_decision".into(), input: json!({ "id": "d" }) },
            ],
        };
        let out = message(&m).unwrap();
        assert_eq!(out.role(), &ConversationRole::Assistant);
        assert_eq!(out.content().len(), 3, "another format's block is left out");
        assert!(matches!(out.content()[0], ContentBlock::ReasoningContent(_)));
        assert!(matches!(&out.content()[2], ContentBlock::ToolUse(u) if u.name() == "explain_decision"));
    }

    /// Calls Bedrock for real when AWZY_TEST_BEDROCK_MODEL names a model this machine's AWS
    /// credentials can use, in AWZY_TEST_BEDROCK_REGION (default us-east-2); skipped otherwise.
    #[tokio::test]
    async fn answers_through_bedrock() {
        let Ok(model) = std::env::var("AWZY_TEST_BEDROCK_MODEL") else {
            eprintln!("skipping: AWZY_TEST_BEDROCK_MODEL is not set");
            return;
        };
        let region = std::env::var("AWZY_TEST_BEDROCK_REGION").unwrap_or_else(|_| "us-east-2".into());
        let request = ModelRequest {
            system: String::new(),
            messages: vec![Message::user("Reply with the single word: ready")],
            tools: vec![],
            max_tokens: 2048,
        };
        let spec = ConverseSpec {
            model_id: model,
            cache_points: false,
            temperature: None,
            additional_model_request_fields: Map::new(),
        };
        let r = respond(&client(&region).await, &spec, &request).await.unwrap();
        assert_eq!(r.stop, StopReason::EndTurn);
        assert!(r.text().to_lowercase().contains("ready"), "{}", r.text());
    }
}
