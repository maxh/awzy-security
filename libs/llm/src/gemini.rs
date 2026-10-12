//! Gemini's `generateContent`: Google's models on Vertex AI.
//!
//! Gemini names a tool result by the tool's name, not a call ID, and ties its reasoning to each
//! call with a thought signature that must come back on that call. Calls get IDs here
//! (`gemini_call_<n>`), and each signature travels as an [`Content::Opaque`] block naming its call.

use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    merge_params, Content, GeminiSpec, LlmError, Message, ModelRequest, ModelResponse, Role, StopReason, Usage,
};

/// This format's name on [`Content::Opaque`] blocks.
pub const FORMAT: &str = "gemini";

/// Call IDs, unique across conversations, so a result always finds its call.
static NEXT_CALL: AtomicU64 = AtomicU64::new(1);

pub fn body(request: &ModelRequest, spec: &GeminiSpec) -> Value {
    let mut body = Map::new();
    if !request.system.is_empty() {
        body.insert("systemInstruction".into(), json!({ "parts": [{ "text": request.system }] }));
    }
    body.insert("contents".into(), Value::Array(request.messages.iter().map(|m| content(m, &request.messages)).collect()));
    if !request.tools.is_empty() {
        let declarations: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "name": t.name, "description": t.description, "parametersJsonSchema": t.input_schema }))
            .collect();
        body.insert("tools".into(), json!([{ "functionDeclarations": declarations }]));
    }
    let mut generation = Map::new();
    generation.insert("maxOutputTokens".into(), json!(request.max_tokens));
    if let Some(t) = spec.temperature {
        generation.insert("temperature".into(), json!(t));
    }
    if let Some(level) = spec.thinking_level {
        generation.insert("thinkingConfig".into(), json!({ "thinkingLevel": level }));
    }
    body.insert("generationConfig".into(), Value::Object(generation));
    let mut body = Value::Object(body);
    merge_params(&mut body, &spec.extra);
    body
}

/// The thought signature Gemini gave call `id`, if any.
fn signature<'a>(m: &'a Message, id: &str) -> Option<&'a Value> {
    m.content.iter().find_map(|c| match c {
        Content::Opaque { format, data } if format == FORMAT && data.get("call").and_then(Value::as_str) == Some(id) => {
            data.get("thoughtSignature")
        }
        _ => None,
    })
}

/// The name of the tool call `id` asked for, from earlier in the conversation.
fn tool_name<'a>(conversation: &'a [Message], id: &str) -> &'a str {
    conversation
        .iter()
        .flat_map(|m| &m.content)
        .find_map(|c| match c {
            Content::ToolUse { id: i, name, .. } if i == id => Some(name.as_str()),
            _ => None,
        })
        .unwrap_or("unknown")
}

fn content(m: &Message, conversation: &[Message]) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "model",
    };
    let parts: Vec<Value> = m
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(json!({ "text": text })),
            Content::ToolUse { id, name, input } => {
                let mut part = Map::new();
                part.insert("functionCall".into(), json!({ "name": name, "args": input }));
                if let Some(sig) = signature(m, id) {
                    part.insert("thoughtSignature".into(), sig.clone());
                }
                Some(Value::Object(part))
            }
            Content::ToolResult { tool_use_id, content, is_error } => {
                let mut response = Map::new();
                response.insert(if *is_error { "error" } else { "content" }.into(), json!(content));
                Some(json!({ "functionResponse": { "name": tool_name(conversation, tool_use_id), "response": response } }))
            }
            // Signatures go on their calls, above; another format's blocks mean nothing here.
            Content::Opaque { .. } => None,
        })
        .collect();
    json!({ "role": role, "parts": parts })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    #[serde(default)]
    candidates: Vec<Candidate>,
    #[serde(default)]
    usage_metadata: WireUsage,
    #[serde(default)]
    prompt_feedback: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    #[serde(default)]
    content: Option<WireContent>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireContent {
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Part {
    text: Option<String>,
    function_call: Option<FunctionCall>,
    /// A summary of its reasoning, not part of the answer.
    #[serde(default)]
    thought: bool,
    thought_signature: Option<Value>,
}

#[derive(Deserialize)]
struct FunctionCall {
    name: String,
    #[serde(default)]
    args: Value,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WireUsage {
    #[serde(default)]
    prompt_token_count: u64,
    #[serde(default)]
    candidates_token_count: u64,
    #[serde(default)]
    thoughts_token_count: u64,
}

pub fn parse(bytes: &[u8], model: &str) -> Result<ModelResponse, LlmError> {
    let wire: Wire = serde_json::from_slice(bytes).map_err(|e| LlmError::Malformed(e.to_string()))?;
    let usage = Usage {
        input_tokens: wire.usage_metadata.prompt_token_count,
        output_tokens: wire.usage_metadata.candidates_token_count + wire.usage_metadata.thoughts_token_count,
    };
    let Some(candidate) = wire.candidates.into_iter().next() else {
        // The prompt itself was blocked.
        let stop = if wire.prompt_feedback.is_some() { StopReason::Refusal } else { StopReason::Other("empty".into()) };
        return Ok(ModelResponse { content: vec![], stop, usage, model: model.to_string() });
    };
    let mut content = Vec::new();
    let mut calls = 0;
    for part in candidate.content.map(|c| c.parts).unwrap_or_default() {
        if let Some(call) = part.function_call {
            calls += 1;
            let id = format!("gemini_call_{}", NEXT_CALL.fetch_add(1, Ordering::Relaxed));
            if let Some(sig) = part.thought_signature {
                content.push(Content::Opaque { format: FORMAT.into(), data: json!({ "call": id, "thoughtSignature": sig }) });
            }
            content.push(Content::ToolUse { id, name: call.name, input: call.args });
        } else if let (Some(text), false) = (part.text, part.thought) {
            content.push(Content::Text { text });
        }
    }
    let stop = match candidate.finish_reason.as_deref() {
        _ if calls > 0 => StopReason::ToolUse,
        Some("STOP") | None => StopReason::EndTurn,
        Some("MAX_TOKENS") => StopReason::MaxTokens,
        Some("SAFETY" | "RECITATION" | "PROHIBITED_CONTENT" | "BLOCKLIST" | "SPII") => StopReason::Refusal,
        Some(other) => StopReason::Other(other.to_string()),
    };
    Ok(ModelResponse { content, stop, usage, model: model.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSpec;

    #[test]
    fn calls_get_ids_and_keep_their_thought_signatures_for_the_next_turn() {
        let r = parse(
            br#"{"candidates":[{"content":{"role":"model","parts":[
                    {"text":"thinking about it","thought":true},
                    {"functionCall":{"name":"explain_decision","args":{"id":"decision_1"}},"thoughtSignature":"sig1"}]},
                  "finishReason":"STOP"}],
                 "usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":20,"thoughtsTokenCount":30}}"#,
            "gemini-pro",
        )
        .unwrap();
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(r.usage, Usage { input_tokens: 100, output_tokens: 50 });
        assert_eq!(r.content.len(), 2, "the thought summary isn't part of the answer: {:?}", r.content);
        let Content::ToolUse { id, .. } = &r.content[1] else { panic!("{:?}", r.content) };
        let id = id.clone();
        assert!(id.starts_with("gemini_call_"));

        // The next turn sends the call back with its signature, and the result by the tool's name.
        let conversation = vec![
            Message::user("Why was bob denied?"),
            Message { role: Role::Assistant, content: r.content },
            Message {
                role: Role::User,
                content: vec![Content::ToolResult { tool_use_id: id, content: "denied by p1".into(), is_error: false }],
            },
        ];
        let request = ModelRequest {
            system: "You are Awzy Bot.".into(),
            messages: conversation,
            tools: vec![ToolSpec { name: "explain_decision".into(), description: "E".into(), input_schema: json!({ "type": "object" }) }],
            max_tokens: 1000,
        };
        let spec: GeminiSpec = serde_json::from_value(json!({ "model": "gemini-2.5-pro", "thinking_level": "low" })).unwrap();
        let body = body(&request, &spec);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "You are Awzy Bot.");
        assert_eq!(body["contents"][1]["role"], "model");
        assert_eq!(body["contents"][1]["parts"][0]["thoughtSignature"], "sig1");
        assert_eq!(body["contents"][1]["parts"][0]["functionCall"]["name"], "explain_decision");
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["name"], "explain_decision");
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["response"]["content"], "denied by p1");
        assert_eq!(body["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]["type"], "object");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 1000);
        assert_eq!(body["generationConfig"]["thinkingConfig"]["thinkingLevel"], "low");
    }

    #[test]
    fn a_blocked_answer_is_a_refusal() {
        let r = parse(br#"{"candidates":[{"finishReason":"SAFETY"}]}"#, "gemini-pro").unwrap();
        assert_eq!(r.stop, StopReason::Refusal);
        let blocked = parse(br#"{"promptFeedback":{"blockReason":"SAFETY"}}"#, "gemini-pro").unwrap();
        assert_eq!(blocked.stop, StopReason::Refusal);
    }
}
