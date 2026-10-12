//! A scripted model for tests: it returns the responses it was given, in order, and remembers
//! every request.

use std::{collections::VecDeque, sync::Mutex};

use serde_json::Value;

use crate::{Content, LlmError, ModelProvider, ModelRequest, ModelResponse, StopReason, Usage};

pub struct FakeModel {
    responses: Mutex<VecDeque<ModelResponse>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl FakeModel {
    pub fn new(responses: impl IntoIterator<Item = ModelResponse>) -> Self {
        Self { responses: Mutex::new(responses.into_iter().collect()), requests: Mutex::default() }
    }

    /// A final text answer.
    pub fn answer(text: &str) -> ModelResponse {
        ModelResponse {
            content: vec![Content::text(text)],
            stop: StopReason::EndTurn,
            usage: Usage::default(),
            model: "fake".into(),
        }
    }

    /// A request to call one tool.
    pub fn tool_call(id: &str, name: &str, input: Value) -> ModelResponse {
        ModelResponse {
            content: vec![Content::ToolUse { id: id.into(), name: name.into(), input }],
            stop: StopReason::ToolUse,
            usage: Usage::default(),
            model: "fake".into(),
        }
    }

    /// Every request made so far.
    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().expect("lock").clone()
    }
}

impl ModelProvider for FakeModel {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        self.requests.lock().expect("lock").push(request.clone());
        self.responses
            .lock()
            .expect("lock")
            .pop_front()
            .ok_or_else(|| LlmError::Failed("the fake model has no more responses".into()))
    }
}
