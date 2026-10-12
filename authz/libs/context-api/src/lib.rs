//! awzy-context's HTTP API as types, and one client for it (#221, #222). Callers get typed values
//! and typed errors, and a field awzy-context renames fails to deserialize instead of reading as
//! empty. The server uses these types too, where it can, and contract tests hold it to the rest.
//!
//! Covered so far: connector settings (ADR 0104), reading resources, and investigations (ADR
//! 0071). The rest of the API moves here area by area.

pub mod model;

use std::time::Duration;

use awzy_http::{encode, Body, Method, Request, StatusCode};
use serde::de::DeserializeOwned;

use crate::model::{
    CloseInvestigation, ConnectorSettings, ConnectorSettingsList, ConnectorSettingsOne, Investigation,
    InvestigationList, InvestigationOne, PutConnectorSettings, Resource, ResourceEntities, ResourceList, ResourceOne,
};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    /// awzy-context refused the request as invalid, and says why.
    #[error("{0}")]
    Invalid(String),
    #[error("awzy-context answered {status}: {body}")]
    Refused { status: u16, body: String },
    #[error("awzy-context's answer to {path} isn't what this client expects: {source}")]
    Unexpected {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("awzy-context is unreachable: {0}")]
    Http(#[from] awzy_http::HttpError),
    #[error("this workload's service token: {0}")]
    Token(String),
}

/// awzy-context's API, with this workload's service token for it (ADR 0055).
#[derive(Clone)]
pub struct ContextApi {
    base: String,
    client: awzy_http::Client,
    tokens: awzy_service_auth::TokenSource,
}

impl ContextApi {
    /// awzy-context at `base`, e.g. `http://awzy-context.awzy-system.svc:8080`.
    pub fn new(base: &str, tokens: awzy_service_auth::TokenSource) -> Self {
        Self { base: base.trim_end_matches('/').to_string(), client: awzy_http::client(), tokens }
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        actor: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> Result<awzy_http::Response, ContextError> {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("{}{path}", self.base))
            .header("content-type", "application/json");
        if let Some(a) = actor {
            req = req.header("x-awzy-actor", a);
        }
        if let Some(authorization) =
            self.tokens.authorization("awzy-context").map_err(|e| ContextError::Token(e.to_string()))?
        {
            req = req.header("authorization", authorization);
        }
        let req = req
            .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
            .map_err(awzy_http::HttpError::from)?;
        Ok(awzy_http::send(&self.client, req, TIMEOUT).await?)
    }

    /// `path`'s answer as `T`; `None` for 404.
    async fn read<T: DeserializeOwned>(&self, path: &str) -> Result<Option<T>, ContextError> {
        let res = self.call(Method::GET, path, None, None).await?;
        if res.status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        decode(path, res).map(Some)
    }

    /// Every connector's settings.
    pub async fn connector_settings(&self) -> Result<Vec<ConnectorSettings>, ContextError> {
        let path = "/v1/connector-settings";
        Ok(self.read::<ConnectorSettingsList>(path).await?.map(|l| l.connectors).unwrap_or_default())
    }

    /// Replaces `name`'s settings, as `actor` (an `Actor`'s string form). `Invalid` says why
    /// awzy-context refused them.
    pub async fn put_connector_settings(
        &self,
        name: &str,
        put: &PutConnectorSettings,
        actor: &str,
    ) -> Result<ConnectorSettings, ContextError> {
        let path = format!("/v1/connector-settings/{}", encode(name));
        let body =
            serde_json::to_value(put).map_err(|source| ContextError::Unexpected { path: path.clone(), source })?;
        let res = self.call(Method::PUT, &path, Some(actor), Some(body)).await?;
        Ok(decode::<ConnectorSettingsOne>(&path, res)?.connector)
    }

    /// One resource; `None` when awzy-context doesn't know it.
    pub async fn resource(&self, id: &str) -> Result<Option<Resource>, ContextError> {
        Ok(self.read::<ResourceOne>(&format!("/v1/resources/{}", encode(id))).await?.map(|r| r.resource))
    }

    /// A resource and the folders above it as Cedar entities, nearest first, and who is assigned
    /// to work on it; `None` when awzy-context doesn't know it.
    pub async fn resource_entities(&self, id: &str) -> Result<Option<ResourceEntities>, ContextError> {
        self.read(&format!("/v1/resources/{}/entities", encode(id))).await
    }

    /// Investigations, newest first, optionally only those with `status`: at most `limit`.
    pub async fn investigations(&self, status: Option<&str>, limit: u32) -> Result<Vec<Investigation>, ContextError> {
        let status = status.map(|s| format!("&status={}", encode(s))).unwrap_or_default();
        let path = format!("/v1/investigations?limit={limit}{status}");
        Ok(self.read::<InvestigationList>(&path).await?.map(|l| l.investigations).unwrap_or_default())
    }

    /// One investigation; `None` when there's no such one.
    pub async fn investigation(&self, qid: &str) -> Result<Option<Investigation>, ContextError> {
        Ok(self
            .read::<InvestigationOne>(&format!("/v1/investigations/{}", encode(qid)))
            .await?
            .map(|i| i.investigation))
    }

    /// Closes `qid` for `reason`, as `actor` (an `Actor`'s string form). `Invalid` says why
    /// awzy-context refused; a 404 or 409 is `Refused` with its reason.
    pub async fn close_investigation(
        &self,
        qid: &str,
        reason: &str,
        actor: &str,
    ) -> Result<Investigation, ContextError> {
        let path = format!("/v1/investigations/{}/close", encode(qid));
        let body = serde_json::to_value(CloseInvestigation { reason: reason.to_string() })
            .map_err(|source| ContextError::Unexpected { path: path.clone(), source })?;
        let res = self.call(Method::POST, &path, Some(actor), Some(body)).await?;
        Ok(decode::<InvestigationOne>(&path, res)?.investigation)
    }

    /// Resources whose ID or name contains `query`, ignoring case: at most `limit`.
    pub async fn search_resources(&self, query: &str, limit: u32) -> Result<Vec<Resource>, ContextError> {
        let path = format!("/v1/resources?limit={limit}&q={}", encode(query));
        Ok(self.read::<ResourceList>(&path).await?.map(|l| l.resources).unwrap_or_default())
    }
}

/// A 2xx answer as `T`; anything else as the error it is.
fn decode<T: DeserializeOwned>(path: &str, res: awzy_http::Response) -> Result<T, ContextError> {
    match res.status.as_u16() {
        200..=299 => serde_json::from_slice(&res.body).map_err(|source| ContextError::Unexpected {
            path: path.split('?').next().unwrap_or(path).to_string(),
            source,
        }),
        400 => Err(ContextError::Invalid(res.text())),
        status => Err(ContextError::Refused { status, body: res.text() }),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        extract::Path,
        response::IntoResponse,
        routing::{get, put},
        Json, Router,
    };
    use serde_json::json;

    use super::*;

    async fn serve(app: Router) -> ContextApi {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        ContextApi::new(&base, awzy_service_auth::TokenSource::off())
    }

    #[tokio::test]
    async fn answers_are_typed_and_refusals_say_why() {
        let api = serve(
            Router::new()
                .route(
                    "/v1/resources/{id}",
                    get(|Path(id): Path<String>| async move {
                        if id == "gone" {
                            return (axum::http::StatusCode::NOT_FOUND, "no such resource").into_response();
                        }
                        Json(json!({ "resource": {
                        "id": id, "source": "google_drive", "kind": "document", "name": "Board deck",
                        "parent": null, "owner": "alice", "sensitivity": "confidential", "labels": [],
                        "sharing": [{ "kind": "group", "id": "board", "role": "reader" }], "text": null, "url": null,
                        "attributes": {}, "created_at": "2026-10-11T12:00:00Z", "updated_at": "2026-10-11T12:00:00Z",
                        "classification": null,
                    }})).into_response()
                    }),
                )
                .route(
                    "/v1/connector-settings/{name}",
                    put(|| async { (axum::http::StatusCode::BAD_REQUEST, "url starts with https://") }),
                ),
        )
        .await;
        let r = api.resource("gdrive:1").await.unwrap().unwrap();
        assert_eq!((r.name.as_str(), r.sharing[0].id.as_str()), ("Board deck", "board"));
        assert!(api.resource("gone").await.unwrap().is_none());
        let put = PutConnectorSettings {
            enabled: true,
            settings: json!({ "url": "http://x" }),
            credential_set: false,
            updated_by: "alice".into(),
        };
        let err = api.put_connector_settings("jira", &put, "Human::alice").await.unwrap_err();
        assert!(matches!(&err, ContextError::Invalid(m) if m.contains("https://")), "{err}");
    }
}
