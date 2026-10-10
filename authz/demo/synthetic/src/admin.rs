//! `larkspur-admin`: a small MCP server standing in for a client's directory and infrastructure
//! administration tools, for the demo to put behind awzy-gateway next to larkspur-drive. Its
//! operations are the high-risk ones the suddenly foolish administrator runs (ADR 0064), and IT
//! runs as routine.
//!
//! Each tool takes what it operates on as `id` (a person, a system) and changes nothing: the
//! demo is about whether the call gets through, not what it does. Like larkspur-drive, it does no
//! authorization of its own.

use axum::{http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::{
    activity::ADMIN_TOOLS,
    drive::{tool_error, PROTOCOL_VERSION},
};

pub fn router() -> Router {
    Router::new().route("/admin/mcp", post(mcp))
}

async fn mcp(Json(msg): Json<Value>) -> axum::response::Response {
    match handle(&msg) {
        Some(reply) => Json(reply).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// The reply to one JSON-RPC message, or `None` for a notification.
pub fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let result = match msg["method"].as_str().unwrap_or_default() {
        "initialize" => Ok(json!({
            "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "larkspur-admin", "version": env!("CARGO_PKG_VERSION") },
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => Ok(call(&msg["params"])),
        other => Err(json!({ "code": -32601, "message": format!("method not found: {other}") })),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
    })
}

fn description(tool: &str) -> &'static str {
    match tool {
        "reset_password" => "Reset a person's password, by their user name.",
        "disable_mfa" => "Turn off multi-factor authentication for a person, by their user name.",
        "rotate_admin_credentials" => "Rotate an administrative credential, by the account's name.",
        "export_directory" => "Export every person, group, and credential setting in the directory.",
        _ => "",
    }
}

fn tools() -> Value {
    let id = json!({ "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] });
    ADMIN_TOOLS
        .iter()
        .map(|(name, _)| json!({ "name": name, "description": description(name), "inputSchema": id }))
        .collect()
}

fn call(params: &Value) -> Value {
    let Some(id) = params["arguments"]["id"].as_str() else { return tool_error("an `id` argument is required") };
    let name = params["name"].as_str().unwrap_or_default();
    if !ADMIN_TOOLS.iter().any(|(t, _)| *t == name) {
        return tool_error(&format!("no tool named {name:?}"));
    }
    json!({ "content": [{ "type": "text", "text": format!("{name} {id}: done (synthetic; nothing changed)") }] })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_msg(tool: &str, id: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": tool, "arguments": { "id": id } } })
    }

    #[test]
    fn lists_and_runs_the_admin_tools() {
        let list = handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" })).unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), ADMIN_TOOLS.len());
        assert!(tools.iter().all(|t| !t["description"].as_str().unwrap().is_empty()));

        let done = handle(&call_msg("export_directory", "directory")).unwrap();
        assert!(done["result"]["content"][0]["text"].as_str().unwrap().contains("nothing changed"));
        assert_eq!(handle(&call_msg("drop_tables", "x")).unwrap()["result"]["isError"], true);
        assert!(handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
    }
}
