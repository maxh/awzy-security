//! OpenAI's chat completions, the format most model servers speak: OpenAI and Azure OpenAI,
//! vLLM, Ollama, and other self-hosted servers, and the open models on Vertex AI.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{
    merge_params, Content, LlmError, Message, ModelRequest, ModelResponse, OpenaiSpec, Role, StopReason, Usage,
};

pub fn body(request: &ModelRequest, spec: &OpenaiSpec) -> Value {
    let mut messages = Vec::new();
    if !request.system.is_empty() {
        messages.push(json!({ "role": "system", "content": request.system }));
    }
    for m in &request.messages {
        push_message(&mut messages, m);
    }
    let mut body = Map::new();
    body.insert("model".into(), json!(spec.model));
    body.insert("messages".into(), Value::Array(messages));
    body.insert("max_completion_tokens".into(), json!(request.max_tokens));
    if let Some(effort) = spec.reasoning_effort {
        body.insert("reasoning_effort".into(), json!(effort));
    }
    if let Some(t) = spec.temperature {
        body.insert("temperature".into(), json!(t));
    }
    if !request.tools.is_empty() {
        let tools: Vec<Value> = request
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.input_schema } }))
            .collect();
        body.insert("tools".into(), Value::Array(tools));
    }
    let mut body = Value::Object(body);
    merge_params(&mut body, &spec.extra);
    body
}

/// A message as chat completions has it: an assistant's text and tool calls in one message, and
/// each tool result in a message of its own.
fn push_message(out: &mut Vec<Value>, m: &Message) {
    let text: String = m
        .content
        .iter()
        .filter_map(|c| match c {
            Content::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    match m.role {
        Role::Assistant => {
            let calls: Vec<Value> = m
                .content
                .iter()
                .filter_map(|c| match c {
                    Content::ToolUse { id, name, input } => Some(json!({
                        "id": id, "type": "function", "function": { "name": name, "arguments": input.to_string() },
                    })),
                    _ => None,
                })
                .collect();
            let mut message = Map::new();
            message.insert("role".into(), json!("assistant"));
            message.insert("content".into(), if text.is_empty() { Value::Null } else { json!(text) });
            if !calls.is_empty() {
                message.insert("tool_calls".into(), Value::Array(calls));
            }
            out.push(Value::Object(message));
        }
        Role::User => {
            for c in &m.content {
                if let Content::ToolResult { tool_use_id, content, is_error } = c {
                    let content = if *is_error { format!("Error: {content}") } else { content.clone() };
                    out.push(json!({ "role": "tool", "tool_call_id": tool_use_id, "content": content }));
                }
            }
            if !text.is_empty() {
                out.push(json!({ "role": "user", "content": text }));
            }
        }
    }
}

#[derive(Deserialize)]
struct Wire {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: WireUsage,
}

#[derive(Deserialize)]
struct Choice {
    message: WireMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct WireMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<WireCall>,
    /// Set when the model declined.
    refusal: Option<String>,
}

#[derive(Deserialize)]
struct WireCall {
    id: String,
    function: WireFunction,
}

#[derive(Deserialize)]
struct WireFunction {
    name: String,
    /// JSON, as a string.
    arguments: String,
}

#[derive(Deserialize, Default)]
struct WireUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

pub fn parse(bytes: &[u8], model: &str) -> Result<ModelResponse, LlmError> {
    let wire: Wire = serde_json::from_slice(bytes).map_err(|e| LlmError::Malformed(e.to_string()))?;
    let usage = Usage { input_tokens: wire.usage.prompt_tokens, output_tokens: wire.usage.completion_tokens };
    let choice =
        wire.choices.into_iter().next().ok_or_else(|| LlmError::Malformed("the answer has no choices".into()))?;
    let mut content = Vec::new();
    if let Some(text) = choice.message.content.filter(|t| !t.is_empty()) {
        content.push(Content::Text { text });
    }
    for call in choice.message.tool_calls {
        let input = serde_json::from_str(&call.function.arguments)
            .map_err(|e| LlmError::Malformed(format!("tool call {}'s arguments: {e}", call.function.name)))?;
        content.push(Content::ToolUse { id: call.id, name: call.function.name, input });
    }
    let stop = match (choice.message.refusal, choice.finish_reason.as_deref()) {
        (Some(_), _) | (_, Some("content_filter")) => StopReason::Refusal,
        (_, Some("tool_calls" | "function_call")) => StopReason::ToolUse,
        (_, Some("length")) => StopReason::MaxTokens,
        (_, Some("stop") | None) => StopReason::EndTurn,
        (_, Some(other)) => StopReason::Other(other.to_string()),
    };
    Ok(ModelResponse { content, stop, usage, model: model.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSpec;

    #[test]
    fn a_tool_loop_round_trips_through_chat_completions() {
        let r = parse(
            br#"{"choices":[{"message":{"role":"assistant","content":null,
                  "tool_calls":[{"id":"call_1","type":"function","function":{"name":"explain_decision","arguments":"{\"id\":\"decision_1\"}"}}]},
                  "finish_reason":"tool_calls"}],
                 "usage":{"prompt_tokens":100,"completion_tokens":20}}"#,
            "llama",
        )
        .unwrap();
        assert_eq!(r.stop, StopReason::ToolUse);
        assert_eq!(
            r.content,
            vec![Content::ToolUse {
                id: "call_1".into(),
                name: "explain_decision".into(),
                input: json!({ "id": "decision_1" })
            }]
        );
        assert_eq!(r.usage, Usage { input_tokens: 100, output_tokens: 20 });

        let request = ModelRequest {
            system: "You are Awzy Bot.".into(),
            messages: vec![
                Message::user("Why was bob denied?"),
                Message { role: Role::Assistant, content: r.content },
                Message {
                    role: Role::User,
                    content: vec![Content::ToolResult {
                        tool_use_id: "call_1".into(),
                        content: "denied by p1".into(),
                        is_error: false,
                    }],
                },
            ],
            tools: vec![ToolSpec {
                name: "explain_decision".into(),
                description: "E".into(),
                input_schema: json!({ "type": "object" }),
            }],
            max_tokens: 1000,
        };
        let spec: OpenaiSpec = serde_json::from_value(
            json!({ "model": "meta/llama-4", "reasoning_effort": "low", "extra": { "top_p": 0.9 } }),
        )
        .unwrap();
        let body = body(&request, &spec);
        assert_eq!(body["model"], "meta/llama-4");
        assert_eq!(body["messages"][0], json!({ "role": "system", "content": "You are Awzy Bot." }));
        assert_eq!(body["messages"][2]["tool_calls"][0]["function"]["arguments"], "{\"id\":\"decision_1\"}");
        assert_eq!(body["messages"][3], json!({ "role": "tool", "tool_call_id": "call_1", "content": "denied by p1" }));
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
        assert_eq!(body["reasoning_effort"], "low");
        assert_eq!(body["top_p"], 0.9);
    }

    #[test]
    fn a_declined_answer_is_a_refusal() {
        let r = parse(
            br#"{"choices":[{"message":{"content":null,"refusal":"I can't help with that."},"finish_reason":"stop"}]}"#,
            "gpt",
        )
        .unwrap();
        assert_eq!(r.stop, StopReason::Refusal);
    }
}
