//! Claude's Messages API: the request body and the response, the same on every cloud's model
//! service but for the version tag each one asks for.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::{Content, LlmError, ModelRequest, ModelResponse, StopReason, Usage};

/// Claude's Messages API body, as a cloud's model service takes it: `version` is the service's
/// `anthropic_version` tag. The system prompt and tools come first and are marked for caching:
/// they're the same on every turn of a conversation.
pub(crate) fn request_body(request: &ModelRequest, version: &str, effort: Option<&str>) -> Value {
    let mut body = json!({
        "anthropic_version": version,
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

pub(crate) fn parse_response(bytes: &[u8]) -> Result<ModelResponse, LlmError> {
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
        let body = request_body(&request, "bedrock-2023-05-31", Some("medium"));
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

        let bare =
            request_body(&ModelRequest { system: String::new(), tools: vec![], ..request }, "vertex-2023-10-16", None);
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
}
