//! The tool loop: send the conversation, run the tools the model asks for, send their results,
//! and repeat until it answers. The conversation is only ever appended to, as the model
//! requires.

use std::future::Future;

use serde_json::Value;

use crate::{
    Content, LlmError, Message, ModelProvider, ModelRequest, ModelResponse, Role, StopReason, ToolSpec, Usage,
};

/// What running a tool produced: text for the model, or an error it should see and work with.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolOutcome {
    Ok(String),
    Error(String),
}

/// Something the model may call. Callers implement it over their own operations, each checked
/// as the person the model is working for.
pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn call(&self, input: Value) -> impl Future<Output = ToolOutcome> + Send;
}

/// How a tool loop ended.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolRun {
    /// The whole conversation, ready to continue.
    pub messages: Vec<Message>,
    /// The model's last response.
    pub last: ModelResponse,
    /// Every tool call made, by name, in order.
    pub calls: Vec<String>,
    pub usage: Usage,
}

/// Runs the conversation until the model stops asking for tools, or for `max_rounds` rounds.
/// `tools` are dispatched by name; a name the model invents gets an error result.
pub async fn run_with_tools<P, T>(
    provider: &P,
    system: &str,
    mut messages: Vec<Message>,
    tools: &[T],
    max_tokens: u32,
    max_rounds: usize,
) -> Result<ToolRun, LlmError>
where
    P: ModelProvider,
    T: Tool,
{
    let specs: Vec<ToolSpec> = tools.iter().map(Tool::spec).collect();
    let mut calls = Vec::new();
    let mut usage = Usage::default();
    for _ in 0..max_rounds.max(1) {
        let request =
            ModelRequest { system: system.to_string(), messages: messages.clone(), tools: specs.clone(), max_tokens };
        let response = provider.respond(&request).await?;
        usage += response.usage;
        messages.push(Message { role: Role::Assistant, content: response.content.clone() });
        if response.stop != StopReason::ToolUse {
            return Ok(ToolRun { messages, last: response, calls, usage });
        }
        // Every tool the model asked for this round, answered together in one message.
        let mut results = Vec::new();
        for block in &response.content {
            let Content::ToolUse { id, name, input } = block else { continue };
            calls.push(name.clone());
            let outcome = match tools.iter().find(|t| t.spec().name == *name) {
                Some(tool) => tool.call(input.clone()).await,
                None => ToolOutcome::Error(format!("there is no tool called {name}")),
            };
            let (content, is_error) = match outcome {
                ToolOutcome::Ok(s) => (s, false),
                ToolOutcome::Error(s) => (s, true),
            };
            results.push(Content::ToolResult { tool_use_id: id.clone(), content, is_error });
        }
        messages.push(Message { role: Role::User, content: results });
    }
    Err(LlmError::Failed(format!("the model was still calling tools after {max_rounds} rounds")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeModel;
    use serde_json::json;

    struct Explain;

    impl Tool for Explain {
        fn spec(&self) -> ToolSpec {
            ToolSpec {
                name: "explain_decision".into(),
                description: "Why a decision came out the way it did".into(),
                input_schema: json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] }),
            }
        }

        async fn call(&self, input: Value) -> ToolOutcome {
            match input["id"].as_str() {
                Some("decision_1") => ToolOutcome::Ok("denied by contractors-no-confidential".into()),
                _ => ToolOutcome::Error("no such decision".into()),
            }
        }
    }

    #[tokio::test]
    async fn tools_run_until_the_model_answers() {
        let model = FakeModel::new([
            FakeModel::tool_call("toolu_1", "explain_decision", json!({ "id": "decision_1" })),
            FakeModel::answer("Bob is a contractor, and the policy keeps contractors out of confidential documents."),
        ]);
        let run = run_with_tools(
            &model,
            "You are Awzy Bot.",
            vec![Message::user("Why was bob denied?")],
            &[Explain],
            4096,
            5,
        )
        .await
        .unwrap();
        assert_eq!(run.calls, ["explain_decision"]);
        assert!(run.last.text().starts_with("Bob is a contractor"));
        // user, assistant (tool call), user (result), assistant (answer)
        assert_eq!(run.messages.len(), 4);
        let Content::ToolResult { content, is_error, .. } = &run.messages[2].content[0] else { panic!() };
        assert_eq!((content.as_str(), *is_error), ("denied by contractors-no-confidential", false));
        // The second request carried the whole conversation so far.
        assert_eq!(model.requests()[1].messages.len(), 3);
    }

    #[tokio::test]
    async fn an_unknown_tool_is_an_error_result_not_a_failure() {
        let model = FakeModel::new([
            FakeModel::tool_call("toolu_1", "publish_policy", json!({})),
            FakeModel::answer("I can't publish policies; I can only draft them."),
        ]);
        let run = run_with_tools(&model, "", vec![Message::user("publish it")], &[Explain], 4096, 5).await.unwrap();
        let Content::ToolResult { is_error, .. } = &run.messages[2].content[0] else { panic!() };
        assert!(is_error);
    }

    #[tokio::test]
    async fn a_model_that_never_stops_calling_tools_is_cut_off() {
        let call = || FakeModel::tool_call("toolu", "explain_decision", json!({ "id": "decision_1" }));
        let model = FakeModel::new([call(), call(), call()]);
        let err = run_with_tools(&model, "", vec![Message::user("loop")], &[Explain], 4096, 3).await.unwrap_err();
        assert!(matches!(err, LlmError::Failed(_)));
    }
}
