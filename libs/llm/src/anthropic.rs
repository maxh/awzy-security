//! Claude's Messages API: Claude on Vertex AI (`rawPredict`) and Anthropic's own API. Claude on
//! Bedrock goes through Converse ([`crate::converse`]) like every Bedrock model.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    merge_params, AnthropicSpec, Content, LlmError, Message, ModelRequest, ModelResponse, Role, StopReason, Usage,
};

/// This format's name on [`Content::Opaque`] blocks.
pub const FORMAT: &str = "anthropic";

/// The request body. `version` is the service's `anthropic_version` (Vertex AI's goes in the body;
/// Anthropic's API takes a header instead), and `named` puts the model in the body (Vertex AI
/// names it in the URL). With prompt caching, the system prompt and tools are marked for caching:
/// they're the same on every turn of a conversation.
pub fn body(request: &ModelRequest, version: Option<&str>, named: bool, spec: &AnthropicSpec) -> Value {
    let cache = spec.prompt_caching;
    let mut body = Map::new();
    if let Some(v) = version {
        body.insert("anthropic_version".into(), json!(v));
    }
    if named {
        body.insert("model".into(), json!(spec.model));
    }
    if let Some(effort) = spec.effort {
        body.insert("output_config".into(), json!({ "effort": effort }));
    }
    if let Some(budget) = spec.thinking_budget_tokens {
        body.insert("thinking".into(), json!({ "type": "enabled", "budget_tokens": budget }));
    }
    body.insert("max_tokens".into(), json!(request.max_tokens));
    body.insert("messages".into(), Value::Array(request.messages.iter().map(message).collect()));
    if !request.system.is_empty() {
        let mut system = json!({ "type": "text", "text": request.system });
        if cache {
            mark_cached(&mut system);
        }
        body.insert("system".into(), json!([system]));
    }
    if !request.tools.is_empty() {
        let mut tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.input_schema }))
            .collect();
        if let (true, Some(last)) = (cache, tools.last_mut()) {
            mark_cached(last);
        }
        body.insert("tools".into(), Value::Array(tools));
    }
    let mut body = Value::Object(body);
    merge_params(&mut body, &spec.extra);
    body
}

fn mark_cached(block: &mut Value) {
    if let Value::Object(o) = block {
        o.insert("cache_control".into(), json!({ "type": "ephemeral" }));
    }
}

fn message(m: &Message) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content: Vec<Value> = m
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(json!({ "type": "text", "text": text })),
            Content::ToolUse { id, name, input } => {
                Some(json!({ "type": "tool_use", "id": id, "name": name, "input": input }))
            }
            Content::ToolResult { tool_use_id, content, is_error } => Some(
                json!({ "type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error }),
            ),
            Content::Opaque { format, data } => (format == FORMAT).then(|| data.clone()),
        })
        .collect();
    json!({ "role": role, "content": content })
}

#[derive(Deserialize)]
struct Wire {
    content: Vec<Value>,
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

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}

pub fn parse(bytes: &[u8], model: &str) -> Result<ModelResponse, LlmError> {
    let wire: Wire = serde_json::from_slice(bytes).map_err(|e| LlmError::Malformed(e.to_string()))?;
    let content = wire
        .content
        .into_iter()
        .map(|b| match serde_json::from_value::<Block>(b.clone()) {
            Ok(Block::Text { text }) => Content::Text { text },
            Ok(Block::ToolUse { id, name, input }) => Content::ToolUse { id, name, input },
            // Its thinking, and anything newer: kept for the next turn.
            Err(_) => Content::Opaque { format: FORMAT.into(), data: b },
        })
        .collect();
    let u = wire.usage;
    Ok(ModelResponse {
        content,
        stop: stop(wire.stop_reason.as_deref().unwrap_or("end_turn")),
        usage: Usage {
            input_tokens: u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens,
            output_tokens: u.output_tokens,
        },
        model: model.to_string(),
    })
}

fn stop(s: &str) -> StopReason {
    match s {
        "end_turn" | "stop_sequence" => StopReason::EndTurn,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        "refusal" => StopReason::Refusal,
        other => StopReason::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSpec;

    fn request() -> ModelRequest {
        ModelRequest {
            system: "You are Awzy Bot.".into(),
            messages: vec![
                Message::user("Why was bob denied?"),
                Message {
                    role: Role::Assistant,
                    content: vec![
                        Content::Opaque {
                            format: FORMAT.into(),
                            data: json!({ "type": "thinking", "thinking": "", "signature": "s" }),
                        },
                        Content::Opaque { format: "gemini".into(), data: json!({ "thoughtSignature": "g" }) },
                        Content::ToolUse { id: "toolu_1".into(), name: "a".into(), input: json!({}) },
                    ],
                },
            ],
            tools: vec![
                ToolSpec { name: "a".into(), description: "A".into(), input_schema: json!({ "type": "object" }) },
                ToolSpec { name: "b".into(), description: "B".into(), input_schema: json!({ "type": "object" }) },
            ],
            max_tokens: 4096,
        }
    }

    fn spec(v: Value) -> AnthropicSpec {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_body_is_claudes_with_the_stable_parts_cached_and_only_its_own_opaque_blocks() {
        let s = spec(json!({ "model": "claude-opus-5-5", "effort": "medium", "prompt_caching": true,
                             "thinking_budget_tokens": 2048, "extra": { "temperature": 1 } }));
        let body = body(&request(), Some("vertex-2023-10-16"), false, &s);
        assert_eq!(body["anthropic_version"], "vertex-2023-10-16");
        assert!(body.get("model").is_none());
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["tools"][1]["cache_control"]["type"], "ephemeral", "one breakpoint, after the last tool");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert_eq!(body["thinking"], json!({ "type": "enabled", "budget_tokens": 2048 }));
        assert_eq!(body["temperature"], 1);
        let assistant = body["messages"][1]["content"].as_array().unwrap();
        assert_eq!(assistant.len(), 2, "another format's block is left out: {assistant:?}");
        assert_eq!(assistant[0]["type"], "thinking");

        let plain = super::body(&request(), None, true, &spec(json!({ "model": "claude-x" })));
        assert_eq!(plain["model"], "claude-x");
        assert!(plain.get("output_config").is_none() && plain.get("thinking").is_none());
        assert!(plain["system"][0].get("cache_control").is_none());
    }

    #[test]
    fn responses_keep_every_block_and_count_cached_input() {
        let r = parse(
            br#"{"content":[{"type":"thinking","thinking":"","signature":"sig"},
                            {"type":"tool_use","id":"toolu_1","name":"explain_decision","input":{"id":"decision_1"}}],
                 "stop_reason":"tool_use",
                 "usage":{"input_tokens":12,"output_tokens":40,"cache_read_input_tokens":900}}"#,
            "claude",
        )
        .unwrap();
        assert_eq!(r.stop, StopReason::ToolUse);
        assert!(matches!(&r.content[0], Content::Opaque { format, .. } if format == FORMAT));
        assert!(matches!(r.content[1], Content::ToolUse { .. }));
        assert_eq!(r.usage, Usage { input_tokens: 912, output_tokens: 40 });
        assert_eq!(r.model, "claude");
        let refused = parse(br#"{"content":[],"stop_reason":"refusal"}"#, "claude").unwrap();
        assert_eq!(refused.stop, StopReason::Refusal);
    }
}
