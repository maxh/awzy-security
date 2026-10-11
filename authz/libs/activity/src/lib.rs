//! awzy-activity: what principals do through awzy's own surfaces, recorded as events in
//! awzy-intake (ADR 0084), so awzy-reasoner scores an agent's tool calls through awzy-gateway or
//! the client portal's MCP endpoint like any other activity.
//!
//! Best effort: an event is queued and sent in the background, a batch a second, and dropped when
//! the queue is full or awzy-intake can't be reached. Recording never slows or fails the call.

use std::time::Duration;

use awzy_model::EntityRef;
use bytes::Bytes;
use chrono::{DateTime, SecondsFormat, Utc};
use http_body_util::Full;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::{json, Map, Value};
use tokio::sync::mpsc;

/// Events queued before new ones are dropped.
const QUEUE: usize = 10_000;
/// Most events in one request; awzy-intake takes up to 1,000.
const BATCH: usize = 500;
const EVERY: Duration = Duration::from_secs(1);

/// One thing a principal did, in awzy-intake's app event schema.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub at: DateTime<Utc>,
    pub principal: EntityRef,
    pub on_behalf_of: Option<EntityRef>,
    pub action: String,
    pub resource: Option<EntityRef>,
    /// success or failure.
    pub outcome: Option<&'static str>,
    /// Anything else worth keeping, such as the MCP client's name. No argument values.
    pub attributes: Map<String, Value>,
}

impl Activity {
    /// `principal` (acting for `on_behalf_of`, if anyone) did `action` on `resource`, now.
    pub fn now(
        principal: EntityRef,
        on_behalf_of: Option<EntityRef>,
        action: &str,
        resource: Option<EntityRef>,
    ) -> Self {
        Self {
            at: Utc::now(),
            principal,
            on_behalf_of,
            action: action.into(),
            resource,
            outcome: None,
            attributes: Map::new(),
        }
    }

    pub fn with_outcome(mut self, outcome: Option<&'static str>) -> Self {
        self.outcome = outcome;
        self
    }

    pub fn with_attribute(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

/// Sends activity to awzy-intake. Cheap to clone.
#[derive(Clone, Default)]
pub struct ActivityLog {
    tx: Option<mpsc::Sender<Activity>>,
}

impl ActivityLog {
    /// Records nothing: no awzy-intake.
    pub fn off() -> Self {
        Self { tx: None }
    }

    /// AWZY_INTAKE_URL (awzy-intake's base URL) and AWZY_INTAKE_TOKEN; off when either is unset.
    /// `system` names the surface in each event, e.g. `awzy-gateway`. Starts the sender, so call
    /// it inside the runtime.
    pub fn from_env(system: &str) -> Self {
        match (std::env::var("AWZY_INTAKE_URL"), std::env::var("AWZY_INTAKE_TOKEN")) {
            (Ok(url), Ok(token)) if !url.is_empty() && !token.is_empty() => Self::new(&url, &token, system),
            _ => {
                tracing::warn!(system, "AWZY_INTAKE_URL or AWZY_INTAKE_TOKEN is not set; activity isn't recorded");
                Self::off()
            }
        }
    }

    pub fn new(url: &str, token: &str, system: &str) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE);
        let endpoint = format!("{}/v1/events", url.trim_end_matches('/'));
        tokio::spawn(send(rx, endpoint, format!("Bearer {token}"), system.to_string()));
        Self { tx: Some(tx) }
    }

    /// Queues `activity` to be sent. Never waits.
    pub fn record(&self, activity: Activity) {
        if let Some(tx) = &self.tx {
            if tx.try_send(activity).is_err() {
                tracing::warn!("the activity queue is full or closed; an event was dropped");
            }
        }
    }
}

fn entity(r: &EntityRef) -> Value {
    json!({ "type": r.kind, "id": r.id })
}

/// The body awzy-intake's `POST /v1/events` takes.
pub fn batch_json(events: &[Activity], system: &str) -> Value {
    let events: Vec<Value> = events
        .iter()
        .map(|a| {
            let mut e = json!({
                "at": a.at.to_rfc3339_opts(SecondsFormat::Millis, true),
                "system": system,
                "principal": entity(&a.principal),
                "action": a.action,
                "attributes": a.attributes,
            });
            if let Some(h) = &a.on_behalf_of {
                e["on_behalf_of"] = entity(h);
            }
            if let Some(r) = &a.resource {
                e["resource"] = entity(r);
            }
            if let Some(o) = a.outcome {
                e["outcome"] = json!(o);
            }
            e
        })
        .collect();
    json!({ "events": events })
}

async fn send(mut rx: mpsc::Receiver<Activity>, endpoint: String, authorization: String, system: String) {
    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let mut batch = Vec::with_capacity(BATCH);
    loop {
        let first = tokio::select! {
            a = rx.recv() => a,
            _ = tokio::time::sleep(EVERY) => None,
        };
        let closed = first.is_none() && rx.is_closed();
        batch.extend(first);
        while batch.len() < BATCH {
            match rx.try_recv() {
                Ok(a) => batch.push(a),
                Err(_) => break,
            }
        }
        if !batch.is_empty() {
            post(&client, &endpoint, &authorization, batch_json(&batch, &system).to_string(), batch.len()).await;
            batch.clear();
        }
        if closed {
            return;
        }
    }
}

async fn post(
    client: &Client<HttpConnector, Full<Bytes>>,
    endpoint: &str,
    authorization: &str,
    body: String,
    n: usize,
) {
    let req = hyper::Request::post(endpoint)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .header(hyper::header::AUTHORIZATION, authorization)
        .body(Full::new(Bytes::from(body)));
    let req = match req {
        Ok(r) => r,
        Err(e) => return tracing::warn!(error = %e, "couldn't build the activity request"),
    };
    match tokio::time::timeout(Duration::from_secs(10), client.request(req)).await {
        Ok(Ok(res)) if res.status().is_success() => {}
        Ok(Ok(res)) => tracing::warn!(status = %res.status(), n, "awzy-intake refused activity"),
        Ok(Err(e)) => tracing::warn!(error = %e, n, "couldn't send activity to awzy-intake"),
        Err(_) => tracing::warn!(n, "awzy-intake didn't answer in time"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{extract::State, http::HeaderMap, routing::post, Json, Router};

    use super::*;

    fn bob_calls(tool: &str) -> Activity {
        Activity {
            at: DateTime::from_timestamp(1_800_000_000, 0).unwrap(),
            principal: EntityRef::new("ServiceAccount", "sales-genie"),
            on_behalf_of: Some(EntityRef::new("Human", "bob")),
            action: "call".into(),
            resource: Some(EntityRef::new("McpTool", tool)),
            outcome: Some("success"),
            attributes: [("mcp_client".to_string(), json!("claude-code"))].into_iter().collect(),
        }
    }

    #[test]
    fn events_are_in_intakes_app_schema() {
        let body = batch_json(&[bob_calls("drive/read_file")], "awzy-gateway");
        assert_eq!(
            body,
            json!({ "events": [{
                "at": "2027-01-15T08:00:00.000Z", "system": "awzy-gateway",
                "principal": { "type": "ServiceAccount", "id": "sales-genie" },
                "on_behalf_of": { "type": "Human", "id": "bob" },
                "action": "call", "resource": { "type": "McpTool", "id": "drive/read_file" },
                "outcome": "success", "attributes": { "mcp_client": "claude-code" },
            }] })
        );
    }

    #[tokio::test]
    async fn activity_reaches_intake_in_batches_with_its_token() {
        type Seen = Arc<Mutex<Vec<(String, Value)>>>;
        let seen: Seen = Arc::default();
        let app = Router::new()
            .route(
                "/v1/events",
                post(|State(seen): State<Seen>, headers: HeaderMap, Json(body): Json<Value>| async move {
                    let auth = headers["authorization"].to_str().unwrap().to_string();
                    seen.lock().unwrap().push((auth, body));
                    "ok"
                }),
            )
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let log = ActivityLog::new(&url, "t0k3n", "awzy-gateway");
        log.record(bob_calls("drive/read_file"));
        log.record(bob_calls("drive/search"));
        for _ in 0..50 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].0, "Bearer t0k3n");
        let tools: Vec<_> = seen
            .iter()
            .flat_map(|(_, b)| b["events"].as_array().unwrap().clone())
            .map(|e| e["resource"]["id"].clone())
            .collect();
        assert_eq!(tools, [json!("drive/read_file"), json!("drive/search")]);
        ActivityLog::off().record(bob_calls("nowhere"));
    }
}
