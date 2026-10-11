//! The install's status page: whether each of awzy's services is ready, and which release it
//! runs, as the portal finds them. For an admin who can't see the cluster, a first look at
//! what's wrong (platform plan, "Operating model").

use std::time::Duration;

use async_graphql::{Context, Object, Result, SimpleObject};
use bytes::Bytes;
use http_body_util::{BodyExt, Empty};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};

use crate::{
    access::{require, PortalAction, PortalArea},
    Services,
};

/// How long the portal waits on each service.
const TIMEOUT: Duration = Duration::from_secs(3);

/// The docs site's troubleshooting guide, with a section per component.
const RUNBOOKS: &str = "https://docs.awzysecurity.com/guides/troubleshooting/";

type HttpClient = Client<HttpConnector, Empty<Bytes>>;

/// The services the portal reaches, by the setting that holds each one's address.
const SERVICES: &[(&str, &str)] = &[
    ("awzy-pdp", "AWZY_PDP_ADDR"),
    ("awzy-policy", "AWZY_POLICY_ADDR"),
    ("awzy-context", "AWZY_CONTEXT_ADDR"),
    ("awzy-challenge", "AWZY_CHALLENGE_ADDR"),
];

#[derive(SimpleObject, Debug, Clone, PartialEq)]
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

#[derive(Default)]
pub struct StatusQuery;

#[Object]
impl StatusQuery {
    /// Whether each of awzy's services the portal uses is ready, and the release it runs. For
    /// those who may view the enforcement area.
    async fn install_status(&self, ctx: &Context<'_>) -> Result<Vec<ComponentStatus>> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        let services = ctx.data::<Services>()?;
        let client: HttpClient = Client::builder(TokioExecutor::new()).build_http();
        let checks = SERVICES.iter().map(|(name, var)| {
            let client = client.clone();
            async move { service_status(&client, name, std::env::var(var).ok()).await }
        });
        let mut status = async_graphql::futures_util::future::join_all(checks).await;
        status.push(clickhouse_status(services.clickhouse.as_ref()).await);
        Ok(status)
    }
}

async fn get(client: &HttpClient, url: &str) -> anyhow::Result<(u16, Bytes)> {
    let req = axum::http::Request::get(url).body(Empty::new())?;
    // The body too: a service that answers and then stalls mustn't hold up the page.
    tokio::time::timeout(TIMEOUT, async {
        let res = client.request(req).await?;
        let status = res.status().as_u16();
        Ok::<_, anyhow::Error>((status, res.into_body().collect().await?.to_bytes()))
    })
    .await?
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

        let client: HttpClient = Client::builder(TokioExecutor::new()).build_http();
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
}
