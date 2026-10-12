//! The install's status page: whether each of awzy's services is ready, and which release it
//! runs, as the portal finds them. For an admin who can't see the cluster, a first look at
//! what's wrong (platform plan, "Operating model").

use std::time::Duration;

use async_graphql::{Context, Object, Result, SimpleObject};
use bytes::Bytes;

use awzy_portal::{JsonObject, UtcTimestamp};

use crate::{
    access::{require, PortalAction, PortalArea},
    Services,
};

/// How long the portal waits on each service.
const TIMEOUT: Duration = Duration::from_secs(3);

/// The docs site's troubleshooting guide, with a section per component.
const RUNBOOKS: &str = "https://docs.awzysecurity.com/guides/troubleshooting/";

pub(crate) type HttpClient = awzy_http::Client;

/// The services the portal reaches, by the setting that holds each one's address.
const SERVICES: &[(&str, &str)] = &[
    ("awzy-pdp", "AWZY_PDP_ADDR"),
    ("awzy-policy", "AWZY_POLICY_ADDR"),
    ("awzy-context", "AWZY_CONTEXT_ADDR"),
    ("awzy-challenge", "AWZY_CHALLENGE_ADDR"),
];

#[derive(SimpleObject, serde::Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub name: String,
    /// It answered that it's ready to serve.
    pub ready: bool,
    /// The release it runs, when it said.
    pub version: Option<String>,
    /// What's wrong, when it isn't ready: unreachable, not ready yet, or not configured.
    pub detail: Option<String>,
    /// What to check when it isn't ready.
    pub runbook_url: String,
}

fn runbook_url(name: &str) -> String {
    format!("{RUNBOOKS}#{}", name.to_lowercase())
}

/// What this install sends awzy: the heartbeat (the only thing that leaves it for awzy), whether
/// it's on, and the last one exactly as sent.
#[derive(SimpleObject, Debug, Clone, PartialEq, Default)]
pub struct HeartbeatExport {
    /// False when the install turned reporting off; then nothing is sent.
    pub enabled: bool,
    /// Where heartbeats go.
    pub corp_url: Option<String>,
    pub interval_secs: Option<i64>,
    pub sent_at: Option<UtcTimestamp>,
    /// Whether awzy's corp portal took the last one.
    pub delivered: Option<bool>,
    /// Why the last one wasn't delivered, or why the portal couldn't ask awzy-reporter.
    pub error: Option<String>,
    /// The last heartbeat, as JSON, byte for byte the fields that were sent. None before the
    /// first, which goes out within a minute of awzy-reporter starting.
    pub heartbeat: Option<JsonObject>,
}

#[derive(Default)]
pub struct StatusQuery;

#[Object]
impl StatusQuery {
    /// Whether each of awzy's services the portal uses is ready, and the release it runs. For
    /// those who may view the enforcement area.
    async fn install_status(&self, ctx: &Context<'_>) -> Result<Vec<ComponentStatus>> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        Ok(check(ctx.data::<Services>()?).await)
    }

    /// What this install sends awzy, and the last heartbeat it sent. For those who may view the
    /// enforcement area.
    async fn heartbeat_export(&self, ctx: &Context<'_>) -> Result<HeartbeatExport> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        let client = awzy_http::client();
        Ok(heartbeat_export(&client, std::env::var("AWZY_REPORTER_ADDR").ok()).await)
    }
}

/// Asks awzy-reporter, at `addr`, what it last sent. No address means reporting is off: the chart
/// deploys no reporter then.
async fn heartbeat_export(client: &HttpClient, addr: Option<String>) -> HeartbeatExport {
    let Some(addr) = addr else { return HeartbeatExport::default() };
    let unreachable = |error: String| HeartbeatExport { enabled: true, error: Some(error), ..Default::default() };
    let body = match get(client, &format!("{}/v1/last-heartbeat", addr.trim_end_matches('/'))).await {
        Ok((200, body)) => body,
        Ok((code, _)) => return unreachable(format!("awzy-reporter answered {code}")),
        Err(e) => return unreachable(format!("awzy-reporter is unreachable: {e}")),
    };
    let report = match serde_json::from_slice::<awzy_heartbeat::LastHeartbeat>(&body) {
        Ok(r) => r,
        Err(e) => return unreachable(format!("awzy-reporter's answer isn't what this portal expects: {e}")),
    };
    let last = report.last.as_ref();
    HeartbeatExport {
        enabled: true,
        corp_url: Some(report.corp_url),
        interval_secs: i64::try_from(report.interval_secs).ok(),
        sent_at: last.map(|l| UtcTimestamp(l.heartbeat.sent_at)),
        delivered: last.map(|l| l.delivered),
        error: last.and_then(|l| l.error.clone()),
        heartbeat: last.and_then(|l| serde_json::to_value(&l.heartbeat).ok()).and_then(JsonObject::from_json),
    }
}

/// Every component, checked at once. The diagnostics bundle carries the same.
pub(crate) async fn check(services: &Services) -> Vec<ComponentStatus> {
    let client = awzy_http::client();
    let checks = SERVICES.iter().map(|(name, var)| {
        let client = client.clone();
        async move { service_status(&client, name, std::env::var(var).ok()).await }
    });
    let mut status = async_graphql::futures_util::future::join_all(checks).await;
    status.push(clickhouse_status(services.clickhouse.as_ref()).await);
    status
}

pub(crate) async fn get(client: &HttpClient, url: &str) -> anyhow::Result<(u16, Bytes)> {
    // The whole response: a service that answers and then stalls mustn't hold up the page.
    let res = awzy_http::get(client, url, &[], TIMEOUT).await?;
    Ok((res.status.as_u16(), res.body))
}

async fn service_status(client: &HttpClient, name: &str, addr: Option<String>) -> ComponentStatus {
    let status = |ready: bool, version: Option<String>, detail: Option<String>| ComponentStatus {
        name: name.to_string(),
        ready,
        version,
        detail,
        runbook_url: runbook_url(name),
    };
    let Some(addr) = addr.map(|a| a.trim_end_matches('/').to_string()) else {
        return status(false, None, Some("not configured".into()));
    };
    let version = match get(client, &format!("{addr}/version")).await {
        Ok((200, body)) => serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v["version"].as_str().map(str::to_string)),
        _ => None,
    };
    match get(client, &format!("{addr}/readyz")).await {
        Ok((200, _)) => status(true, version, None),
        Ok((code, _)) => {
            status(false, version, Some(format!("not ready ({code}): starting, or waiting on a dependency")))
        }
        Err(e) => status(false, None, Some(format!("unreachable: {e}"))),
    }
}

async fn clickhouse_status(clickhouse: Option<&::clickhouse::Client>) -> ComponentStatus {
    let status = |ready: bool, detail: Option<String>| ComponentStatus {
        name: "ClickHouse".into(),
        ready,
        version: None,
        detail,
        runbook_url: runbook_url("ClickHouse"),
    };
    let Some(ch) = clickhouse else { return status(false, Some("not configured".into())) };
    match tokio::time::timeout(TIMEOUT, ch.query("SELECT 1").fetch_one::<u8>()).await {
        Ok(Ok(_)) => status(true, None),
        Ok(Err(e)) => status(false, Some(format!("unreachable: {e}"))),
        Err(_) => status(false, Some("didn't answer in time".into())),
    }
}

#[cfg(test)]
mod tests {
    use axum::{routing::get, Router};

    use super::*;

    #[tokio::test]
    async fn a_service_is_ready_unready_unreachable_or_not_configured() {
        let app = Router::new()
            .route("/readyz", get(|| async { "ok" }))
            .route("/version", get(|| async { axum::Json(serde_json::json!({ "version": "0.1.0-gabc" })) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ready = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let starting =
            Router::new().route("/readyz", get(|| async { (axum::http::StatusCode::SERVICE_UNAVAILABLE, "no") }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unready = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, starting).await.unwrap() });

        let client = awzy_http::client();
        let up = service_status(&client, "awzy-pdp", Some(ready)).await;
        assert_eq!((up.ready, up.version.as_deref(), up.detail), (true, Some("0.1.0-gabc"), None));
        let starting = service_status(&client, "awzy-policy", Some(unready)).await;
        assert!(!starting.ready && starting.detail.unwrap().starts_with("not ready (503)"));
        let gone = service_status(&client, "awzy-context", Some("http://127.0.0.1:9".into())).await;
        assert!(!gone.ready && gone.detail.unwrap().starts_with("unreachable"));
        let unset = service_status(&client, "awzy-challenge", None).await;
        assert_eq!(unset.detail.as_deref(), Some("not configured"));
        assert_eq!(unset.runbook_url, "https://docs.awzysecurity.com/guides/troubleshooting/#awzy-challenge");
        assert_eq!(clickhouse_status(None).await.detail.as_deref(), Some("not configured"));
    }

    #[tokio::test]
    async fn the_heartbeat_export_is_off_unreachable_or_the_last_one_sent() {
        let client = awzy_http::client();
        assert_eq!(heartbeat_export(&client, None).await, HeartbeatExport::default(), "no reporter: nothing is sent");
        let gone = heartbeat_export(&client, Some("http://127.0.0.1:9".into())).await;
        assert!(gone.enabled && gone.error.unwrap().starts_with("awzy-reporter is unreachable"));

        let app = Router::new().route(
            "/v1/last-heartbeat",
            get(|| async {
                axum::Json(serde_json::json!({
                    "corp_url": "https://corp.example.com",
                    "interval_secs": 60,
                    "last": { "delivered": true, "error": null, "heartbeat": {
                        "schema_version": 1, "install_qid": "install_00000000-0000-7000-8000-000000000001",
                        "sent_at": "2026-10-11T12:00:00Z", "chart_version": "0.1.0", "services": [],
                    } },
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let reporter = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let sent = heartbeat_export(&client, Some(reporter)).await;
        assert_eq!((sent.enabled, sent.delivered, sent.interval_secs), (true, Some(true), Some(60)));
        assert_eq!(sent.sent_at.unwrap().0.to_rfc3339(), "2026-10-11T12:00:00+00:00");
        assert_eq!(sent.heartbeat.unwrap().0["sent_at"], "2026-10-11T12:00:00Z");
    }
}
