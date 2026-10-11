//! The Connectors page: which systems of record awzy reads (Google Drive, Jira), what each was
//! configured with, and whether it's syncing. Read from awzy-connectors; `connector_config` is
//! where they're configured.

use async_graphql::{Context, Object, Result, SimpleObject};
use awzy_portal::UtcTimestamp;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    access::{require, PortalAction, PortalArea},
    status::{get, HttpClient},
};

/// One of a connector's settings, for display.
#[derive(SimpleObject, Debug, Clone, PartialEq)]
pub struct ConnectorSetting {
    pub name: String,
    pub value: String,
}

/// A configured connector and how its syncing is going.
#[derive(SimpleObject, Debug, Clone, PartialEq)]
pub struct Connector {
    /// drive or jira.
    pub name: String,
    /// What it was configured with. Never its credentials.
    pub settings: Vec<ConnectorSetting>,
    pub interval_secs: i64,
    pub last_success_at: Option<UtcTimestamp>,
    pub last_failure_at: Option<UtcTimestamp>,
    /// Why the last failed round failed; null once a round succeeds.
    pub last_error: Option<String>,
    pub failures: i64,
    /// Written to or deleted from awzy's world model since awzy-connectors started.
    pub written: i64,
    pub deleted: i64,
}

fn time(v: &Value) -> Option<UtcTimestamp> {
    v.as_str().and_then(|t| t.parse::<DateTime<Utc>>().ok()).map(UtcTimestamp)
}

fn connector(c: &Value) -> Connector {
    Connector {
        name: c["name"].as_str().unwrap_or_default().to_string(),
        settings: c["settings"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(name, value)| ConnectorSetting {
                name: name.clone(),
                value: value.as_str().unwrap_or_default().to_string(),
            })
            .collect(),
        interval_secs: c["interval_secs"].as_i64().unwrap_or_default(),
        last_success_at: time(&c["last_success_at"]),
        last_failure_at: time(&c["last_failure_at"]),
        last_error: c["last_error"].as_str().map(str::to_string),
        failures: c["failures"].as_i64().unwrap_or_default(),
        written: c["written"].as_i64().unwrap_or_default(),
        deleted: c["deleted"].as_i64().unwrap_or_default(),
    }
}

/// awzy-connectors' status at `addr`. No address means none is configured: the chart deploys
/// awzy-connectors only when one is.
async fn connectors(client: &HttpClient, addr: Option<String>) -> Result<Vec<Connector>> {
    let Some(addr) = addr else { return Ok(vec![]) };
    let body = match get(client, &format!("{}/v1/status", addr.trim_end_matches('/'))).await {
        Ok((200, body)) => body,
        Ok((code, _)) => return Err(format!("awzy-connectors answered {code}").into()),
        Err(e) => return Err(format!("awzy-connectors is unreachable: {e}").into()),
    };
    let v: Value = serde_json::from_slice(&body)?;
    Ok(v["connectors"].as_array().into_iter().flatten().map(connector).collect())
}

#[derive(Default)]
pub struct ConnectorsQuery;

#[Object]
impl ConnectorsQuery {
    /// The configured connectors and how their syncing is going. For those who may view the
    /// enforcement area.
    async fn connectors(&self, ctx: &Context<'_>) -> Result<Vec<Connector>> {
        require(ctx, PortalAction::View, PortalArea::Enforcement).await?;
        let client = awzy_http::client();
        connectors(&client, std::env::var("AWZY_CONNECTORS_ADDR").ok()).await
    }
}

#[cfg(test)]
mod tests {
    use axum::{routing::get, Router};

    use super::*;

    #[tokio::test]
    async fn connectors_are_none_unreachable_or_as_awzy_connectors_reports_them() {
        let client = awzy_http::client();
        assert!(connectors(&client, None).await.unwrap().is_empty(), "nothing configured");
        let gone = connectors(&client, Some("http://127.0.0.1:9".into())).await.unwrap_err();
        assert!(gone.message.starts_with("awzy-connectors is unreachable"));

        let app = Router::new().route(
            "/v1/status",
            get(|| async {
                axum::Json(serde_json::json!({ "connectors": [{
                    "name": "jira", "settings": { "Site": "https://example.atlassian.net" }, "interval_secs": 300,
                    "last_success_at": "2026-10-11T12:00:00Z", "last_failure_at": null, "last_error": null,
                    "failures": 0, "written": 12, "deleted": 1,
                }]}))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let found = connectors(&client, Some(addr)).await.unwrap();
        assert_eq!(found.len(), 1);
        let jira = &found[0];
        assert_eq!((jira.name.as_str(), jira.written, jira.deleted, jira.interval_secs), ("jira", 12, 1, 300));
        assert_eq!(jira.settings[0].value, "https://example.atlassian.net");
        assert_eq!(jira.last_success_at.unwrap().0.to_rfc3339(), "2026-10-11T12:00:00+00:00");
    }
}
