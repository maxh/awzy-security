//! Writing resources to awzy-context (ADR 0044), with this service's token (ADR 0055).

use std::collections::BTreeSet;

use anyhow::bail;
use axum::http::{header, Method, StatusCode};
use serde_json::{json, Value};

use crate::http::{encode, Body, Http};

/// The most resources one batch carries: awzy-context's limit.
pub const BATCH: usize = 1000;

/// Who the writes are recorded as (ADR 0060).
const ACTOR: &str = "AwzyService::awzy-connectors";

pub struct ContextClient {
    base_url: String,
    http: Http,
    tokens: awzy_service_auth::TokenSource,
}

impl ContextClient {
    pub fn new(base_url: &str, http: Http, tokens: awzy_service_auth::TokenSource) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), http, tokens }
    }

    async fn send(&self, method: Method, path: &str, body: Body<'_>) -> anyhow::Result<(StatusCode, Value)> {
        let mut headers = vec![("x-awzy-actor", ACTOR.to_string())];
        if let Some(authorization) = self.tokens.authorization("awzy-context")? {
            headers.push((header::AUTHORIZATION.as_str(), authorization));
        }
        self.http.send(method, &format!("{}{path}", self.base_url), &headers, body).await
    }

    /// Writes `resources`, replacing any with the same IDs, in batches.
    pub async fn put_resources(&self, resources: &[Value]) -> anyhow::Result<()> {
        for batch in resources.chunks(BATCH) {
            let body = json!({ "resources": batch });
            let (status, res) = self.send(Method::POST, "/v1/resources/batch", Body::Json(&body)).await?;
            if !status.is_success() {
                bail!("awzy-context refused a batch of {} resources ({status}): {res}", batch.len());
            }
        }
        Ok(())
    }

    /// Deletes one resource. Already gone counts as done.
    pub async fn delete_resource(&self, id: &str) -> anyhow::Result<()> {
        let (status, res) = self.send(Method::DELETE, &format!("/v1/resources/{}", encode(id)), Body::None).await?;
        if !status.is_success() && status != StatusCode::NOT_FOUND {
            bail!("awzy-context refused to delete {id} ({status}): {res}");
        }
        Ok(())
    }

    /// Writes `items`, replacing any with the same IDs, in batches.
    pub async fn put_work_items(&self, items: &[Value]) -> anyhow::Result<()> {
        for batch in items.chunks(BATCH) {
            let body = json!({ "work_items": batch });
            let (status, res) = self.send(Method::POST, "/v1/work-items/batch", Body::Json(&body)).await?;
            if !status.is_success() {
                bail!("awzy-context refused a batch of {} work items ({status}): {res}", batch.len());
            }
        }
        Ok(())
    }

    /// Deletes one work item. Already gone counts as done.
    pub async fn delete_work_item(&self, id: &str) -> anyhow::Result<()> {
        let (status, res) = self.send(Method::DELETE, &format!("/v1/work-items/{}", encode(id)), Body::None).await?;
        if !status.is_success() && status != StatusCode::NOT_FOUND {
            bail!("awzy-context refused to delete {id} ({status}): {res}");
        }
        Ok(())
    }

    /// The IDs of every resource from `source`, page by page.
    pub async fn resource_ids(&self, source: &str) -> anyhow::Result<BTreeSet<String>> {
        self.ids("resources", "resources", source).await
    }

    /// The IDs of every work item from `source`, page by page.
    pub async fn work_item_ids(&self, source: &str) -> anyhow::Result<BTreeSet<String>> {
        self.ids("work-items", "work_items", source).await
    }

    async fn ids(&self, path: &str, field: &str, source: &str) -> anyhow::Result<BTreeSet<String>> {
        let mut ids = BTreeSet::new();
        let mut after: Option<String> = None;
        loop {
            let mut path = format!("/v1/{path}?source={}&limit=500", encode(source));
            if let Some(a) = &after {
                path.push_str(&format!("&after={}", encode(a)));
            }
            let (status, res) = self.send(Method::GET, &path, Body::None).await?;
            if !status.is_success() {
                bail!("awzy-context refused to list {source} {field} ({status}): {res}");
            }
            let page: Vec<String> = res[field]
                .as_array()
                .map(|rs| rs.iter().filter_map(|r| r["id"].as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let Some(last) = page.last().cloned() else { return Ok(ids) };
            ids.extend(page);
            after = Some(last);
        }
    }
}
