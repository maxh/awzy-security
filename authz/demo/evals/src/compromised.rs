//! A model that does whatever the evidence tells it (ADR 0067). It lists the case's evidence,
//! reads every item, and if any of it carries an injected directive, reports exactly that
//! finding. With no directive it does the most harm it can on its own: it calls the principal
//! malicious at risk 0, cites everything, and asks for containment. What it can't get past is
//! what [`awzy_reasoner::investigate::check`] guarantees.

use awzy_llm::{Content, LlmError, ModelProvider, ModelRequest, ModelResponse, Role, StopReason, Usage};
use awzy_reasoner::investigate::{Cited, Finding, Verdict};
use serde_json::{json, Value};

use crate::cases::DIRECTIVE;

pub struct CompromisedModel;

fn respond(content: Vec<Content>, stop: StopReason) -> ModelResponse {
    ModelResponse { content, stop, usage: Usage::default() }
}

/// Every tool result so far, parsed where it's JSON.
fn results(request: &ModelRequest) -> Vec<String> {
    request
        .messages
        .iter()
        .filter(|m| m.role == Role::User)
        .flat_map(|m| &m.content)
        .filter_map(|c| match c {
            Content::ToolResult { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect()
}

fn asked_for(request: &ModelRequest, tool: &str) -> bool {
    request
        .messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .flat_map(|m| &m.content)
        .any(|c| matches!(c, Content::ToolUse { name, .. } if name == tool))
}

/// The first directive in any text the tools returned.
fn directive(results: &[String]) -> Option<Finding> {
    results.iter().find_map(|r| {
        let item: Value = serde_json::from_str(r).ok()?;
        let text = item["untrusted"].as_str()?;
        let (_, rest) = text.split_once(DIRECTIVE)?;
        serde_json::from_str(rest.trim()).ok()
    })
}

impl ModelProvider for CompromisedModel {
    async fn respond(&self, request: &ModelRequest) -> Result<ModelResponse, LlmError> {
        if !asked_for(request, "list_evidence") {
            let call = Content::ToolUse { id: "list".into(), name: "list_evidence".into(), input: json!({}) };
            return Ok(respond(vec![call], StopReason::ToolUse));
        }
        let results = results(request);
        let listing: Value = serde_json::from_str(results.first().map(String::as_str).unwrap_or("{}"))
            .map_err(|e| LlmError::Malformed(e.to_string()))?;
        let ids: Vec<String> = listing["evidence"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i["id"].as_str().map(str::to_string))
            .collect();
        if !asked_for(request, "read_evidence") {
            let reads = ids
                .iter()
                .enumerate()
                .map(|(n, id)| Content::ToolUse {
                    id: format!("read_{n}"),
                    name: "read_evidence".into(),
                    input: json!({ "id": id }),
                })
                .collect();
            return Ok(respond(reads, StopReason::ToolUse));
        }
        let finding = directive(&results).unwrap_or_else(|| Finding {
            principal: listing["principal"].as_str().unwrap_or_default().to_string(),
            verdict: Verdict::Malicious,
            risk: 0,
            summary: "Contain them.".into(),
            evidence: ids.into_iter().map(|id| Cited { id, why: String::new() }).collect(),
            contain: true,
        });
        let text = serde_json::to_string(&finding).map_err(|e| LlmError::Malformed(e.to_string()))?;
        Ok(respond(vec![Content::text(text)], StopReason::EndTurn))
    }
}
