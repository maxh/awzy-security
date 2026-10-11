//! awzy-intake: normalizes app logs, cloud audit logs, DLP signals, and agent traces into one event schema on Kafka.
//!
//! So far: app events in awzy's own schema (`POST /v1/events`) and AWS CloudTrail log files
//! (`POST /v1/events/aws-cloudtrail`), each normalized into an `EventRecord` and acknowledged
//! once Kafka has it. Log shippers authenticate with a bearer token the client sets
//! (AWZY_INTAKE_TOKEN). A CloudTrail session of a people role is mapped to its person (ADR 0086).
//! The other clouds' audit logs, DLP signals, and agent traces come next.

pub mod kafka;
pub mod normalize;

use std::{future::Future, sync::Arc};

use awzy_model::EventRecord;
use axum::{
    extract::{rejection::JsonRejection, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::json;
use sha2::{Digest, Sha256};

pub use kafka::{EventLog, DEFAULT_EVENTS_TOPIC};
use normalize::NormalizeError;

/// Where normalized events go. A trait so the routes can be tested without Kafka.
pub trait EventSink: Send + Sync + 'static {
    fn send(&self, events: &[EventRecord]) -> impl Future<Output = anyhow::Result<()>> + Send;
}

impl EventSink for EventLog {
    async fn send(&self, events: &[EventRecord]) -> anyhow::Result<()> {
        Ok(EventLog::send(self, events).await?)
    }
}

struct Intake<S> {
    sink: Arc<S>,
    /// SHA-256 of the shippers' bearer token, compared in constant time.
    token_sha256: [u8; 32],
    /// Which CloudTrail callers are people (ADR 0086).
    humans: Arc<normalize::CloudTrailHumans>,
}

impl<S> Clone for Intake<S> {
    fn clone(&self) -> Self {
        Self { sink: self.sink.clone(), token_sha256: self.token_sha256, humans: self.humans.clone() }
    }
}

/// Routes this service serves in addition to the shared health routes, behind `token`.
pub fn router<S: EventSink>(sink: Arc<S>, token: &str) -> Router {
    let humans = Arc::new(normalize::CloudTrailHumans::from_env());
    let state = Intake { sink, token_sha256: Sha256::digest(token.as_bytes()).into(), humans };
    Router::new()
        .route("/v1/events", post(app_events::<S>))
        .route("/v1/events/aws-cloudtrail", post(cloudtrail_events::<S>))
        .layer(middleware::from_fn_with_state(state.clone(), authenticate::<S>))
        .with_state(state)
}

async fn authenticate<S: EventSink>(State(intake): State<Intake<S>>, req: Request, next: Next) -> Response {
    let presented =
        req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    let ok = presented.is_some_and(|t| {
        let digest: [u8; 32] = Sha256::digest(t.as_bytes()).into();
        digest.iter().zip(intake.token_sha256.iter()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    });
    if !ok {
        return error(StatusCode::UNAUTHORIZED, "a valid intake bearer token is required");
    }
    next.run(req).await
}

async fn app_events<S: EventSink>(
    State(intake): State<Intake<S>>,
    body: Result<Json<normalize::AppBatch>, JsonRejection>,
) -> Response {
    match body {
        Ok(Json(batch)) => accept(&intake, normalize::app(batch, chrono::Utc::now())).await,
        Err(rejection) => error(StatusCode::BAD_REQUEST, rejection.body_text()),
    }
}

async fn cloudtrail_events<S: EventSink>(
    State(intake): State<Intake<S>>,
    body: Result<Json<normalize::CloudTrailBatch>, JsonRejection>,
) -> Response {
    match body {
        Ok(Json(batch)) => accept(&intake, normalize::aws_cloudtrail(batch, chrono::Utc::now(), &intake.humans)).await,
        Err(rejection) => error(StatusCode::BAD_REQUEST, rejection.body_text()),
    }
}

async fn accept<S: EventSink>(intake: &Intake<S>, events: Result<Vec<EventRecord>, NormalizeError>) -> Response {
    let events = match events {
        Ok(events) => events,
        Err(e) => return error(StatusCode::BAD_REQUEST, e),
    };
    match intake.sink.send(&events).await {
        Ok(()) => {
            let qids: Vec<String> = events.iter().map(|e| e.id.to_string()).collect();
            (StatusCode::ACCEPTED, Json(json!({ "accepted": events.len(), "qids": qids }))).into_response()
        }
        Err(e) => {
            tracing::warn!(error = %e, count = events.len(), "events not delivered to Kafka; the sender should retry");
            error(StatusCode::SERVICE_UNAVAILABLE, "events could not be stored; retry")
        }
    }
}

fn error(status: StatusCode, message: impl std::fmt::Display) -> Response {
    (status, Json(json!({ "error": message.to_string() }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use std::sync::Mutex;
    use tower::ServiceExt;

    const TOKEN: &str = "intake-test-token-0123456789abcdef";

    #[derive(Default)]
    struct Recorded {
        events: Mutex<Vec<EventRecord>>,
        fail: bool,
    }

    impl EventSink for Recorded {
        async fn send(&self, events: &[EventRecord]) -> anyhow::Result<()> {
            anyhow::ensure!(!self.fail, "kafka is down");
            self.events.lock().unwrap().extend_from_slice(events);
            Ok(())
        }
    }

    async fn post(
        sink: Arc<Recorded>,
        path: &str,
        token: Option<&str>,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let mut req = Request::post(path).header(header::CONTENT_TYPE, "application/json");
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let res = router(sink, TOKEN).oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or_default())
    }

    fn one_event() -> serde_json::Value {
        json!({ "events": [{ "at": chrono::Utc::now().to_rfc3339(), "system": "jira",
                             "principal": { "type": "Human", "id": "bob" }, "action": "read" }] })
    }

    #[tokio::test]
    async fn accepted_events_reach_the_sink_and_their_ids_come_back() {
        let sink = Arc::new(Recorded::default());
        let (status, body) = post(sink.clone(), "/v1/events", Some(TOKEN), one_event()).await;
        assert_eq!(status, StatusCode::ACCEPTED);
        let stored = sink.events.lock().unwrap().clone();
        assert_eq!(stored.len(), 1);
        assert_eq!(body["qids"][0], stored[0].id.to_string());
    }

    #[tokio::test]
    async fn events_need_the_intake_token() {
        let sink = Arc::new(Recorded::default());
        assert_eq!(post(sink.clone(), "/v1/events", None, one_event()).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(post(sink.clone(), "/v1/events", Some("wrong"), one_event()).await.0, StatusCode::UNAUTHORIZED);
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_batches_are_refused_with_the_reason() {
        let sink = Arc::new(Recorded::default());
        let (status, body) = post(sink.clone(), "/v1/events", Some(TOKEN), json!({ "events": [] })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "the request has no events");
        let (status, _) = post(sink.clone(), "/v1/events/aws-cloudtrail", Some(TOKEN), json!({ "records": [] })).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(sink.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn when_kafka_is_down_the_sender_is_told_to_retry() {
        let sink = Arc::new(Recorded { fail: true, ..Default::default() });
        assert_eq!(post(sink, "/v1/events", Some(TOKEN), one_event()).await.0, StatusCode::SERVICE_UNAVAILABLE);
    }
}
