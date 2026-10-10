//! `larkspur-drive`: a small MCP server over Larkspur's documents in awzy-context, for the demo
//! to put behind awzy-gateway (demo plan step 5, ADR 0050). It stands in for a real client's
//! Google Drive or SharePoint MCP server, so agents' tool calls are about real resources and
//! every one is a PDP decision on the document it touches.
//!
//! Streamable HTTP with JSON responses, and only what an agent needs: `list_folder` and
//! `read_document`, each taking the resource's ID as `id`. It does no authorization of its own:
//! the gateway in front of it is the enforcement point.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::push::ContextClient;

pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";

pub fn router(context: ContextClient) -> Router {
    Router::new().route("/mcp", post(mcp)).with_state(Arc::new(context))
}

async fn mcp(State(context): State<Arc<ContextClient>>, Json(msg): Json<Value>) -> axum::response::Response {
    match handle(&context, &msg).await {
        Some(reply) => Json(reply).into_response(),
        // A notification: nothing to say.
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// The reply to one JSON-RPC message, or `None` for a notification.
pub async fn handle(context: &ContextClient, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let result = match msg["method"].as_str().unwrap_or_default() {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "larkspur-drive", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => Ok(call(context, &msg["params"]).await),
        other => Err(json!({ "code": -32601, "message": format!("method not found: {other}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn tools() -> Value {
    let id = json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] });
    json!([
        { "name": "list_folder", "description": "List the documents and folders in a folder, by the folder's ID.",
          "inputSchema": id },
        { "name": "read_document", "description": "Read a document's title and text, by its ID.",
          "inputSchema": id },
    ])
}

async fn call(context: &ContextClient, params: &Value) -> Value {
    let Some(id) = params["arguments"]["id"].as_str() else { return tool_error("an `id` argument is required") };
    let outcome = match params["name"].as_str().unwrap_or_default() {
        "read_document" => read_document(context, id).await,
        "list_folder" => list_folder(context, id).await,
        other => return tool_error(&format!("no tool named {other:?}")),
    };
    match outcome {
        Ok(text) => json!({ "content": [{ "type": "text", "text": text }] }),
        Err(e) => tool_error(&e.to_string()),
    }
}

async fn read_document(context: &ContextClient, id: &str) -> anyhow::Result<String> {
    let (status, body) = context.get(&format!("/v1/resources/{}", encode(id))).await?;
    anyhow::ensure!(status != StatusCode::NOT_FOUND, "no document {id}");
    anyhow::ensure!(status.is_success(), "awzy-context returned {status}");
    let r = &body["resource"];
    Ok(format!("{}\n\n{}", r["name"].as_str().unwrap_or(id), r["text"].as_str().unwrap_or("")))
}

async fn list_folder(context: &ContextClient, id: &str) -> anyhow::Result<String> {
    let (status, body) = context.get(&format!("/v1/resources?parent={}&limit=200", encode(id))).await?;
    anyhow::ensure!(status.is_success(), "awzy-context returned {status}");
    let lines: Vec<String> = body["resources"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            format!(
                "{} ({}): {}",
                r["id"].as_str().unwrap_or(""),
                r["kind"].as_str().unwrap_or(""),
                r["name"].as_str().unwrap_or("")
            )
        })
        .collect();
    Ok(if lines.is_empty() { format!("{id} is empty") } else { lines.join("\n") })
}

pub(crate) fn tool_error(text: &str) -> Value {
    json!({ "isError": true, "content": [{ "type": "text", "text": text }] })
}

/// Percent-encodes an ID for a path segment or query value: resource IDs may contain `/`.
fn encode(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::{Path, Query},
        routing::get,
    };

    use super::*;

    async fn fake_context() -> ContextClient {
        let app = Router::new()
            .route(
                "/v1/resources/{id}",
                get(|Path(id): Path<String>| async move {
                    if id != "synthetic:doc:00010-000" {
                        return Err(StatusCode::NOT_FOUND);
                    }
                    Ok(Json(json!({ "resource": { "id": id, "name": "Account plan: Harborline Bank",
                        "text": "Confidential: Larkspur internal." } })))
                }),
            )
            .route(
                "/v1/resources",
                get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
                    let in_folder = q.get("parent").map(String::as_str) == Some("synthetic:folder:sales-emea/accounts");
                    let resources = if in_folder {
                        json!([{ "id": "synthetic:doc:00010-000", "kind": "document", "name": "Account plan" }])
                    } else {
                        json!([])
                    };
                    Json(json!({ "resources": resources }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        ContextClient::new(&format!("http://{addr}"), "")
    }

    fn call_msg(tool: &str, id: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": { "id": id } } })
    }

    #[tokio::test]
    async fn speaks_enough_mcp_for_an_agent() {
        let context = fake_context().await;
        let init = handle(&context, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} })).await;
        assert_eq!(init.unwrap()["result"]["serverInfo"]["name"], "larkspur-drive");
        assert!(handle(&context, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await.is_none());
        let list = handle(&context, &json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).await.unwrap();
        assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 2);
        let unknown = handle(&context, &json!({ "jsonrpc": "2.0", "id": 2, "method": "resources/list" })).await;
        assert_eq!(unknown.unwrap()["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn reads_documents_and_lists_folders_from_awzy_context() {
        let context = fake_context().await;
        let read = handle(&context, &call_msg("read_document", "synthetic:doc:00010-000")).await.unwrap();
        let text = read["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("Account plan: Harborline Bank") && text.contains("Confidential"), "{text}");

        let list = handle(&context, &call_msg("list_folder", "synthetic:folder:sales-emea/accounts")).await.unwrap();
        assert!(list["result"]["content"][0]["text"].as_str().unwrap().contains("synthetic:doc:00010-000"));

        let missing = handle(&context, &call_msg("read_document", "synthetic:doc:nope")).await.unwrap();
        assert_eq!(missing["result"]["isError"], true);
    }
}
