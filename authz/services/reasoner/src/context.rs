//! awzy-context, for who is on which team and to write the scores, with this service's token
//! for awzy-context (ADR 0055).

use std::{collections::HashMap, time::Duration};

use anyhow::{bail, Context};
use axum::http::{header, Method, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::{json, Value};

use crate::{activity::Principal, Scored};

/// The most scores one request carries; awzy-context takes up to 10,000.
const BATCH: usize = 1000;
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct ContextClient {
    base_url: String,
    http: Client<HttpConnector, Full<Bytes>>,
    tokens: awzy_service_auth::TokenSource,
}

impl ContextClient {
    pub fn new(base_url: &str, tokens: awzy_service_auth::TokenSource) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: Client::builder(TokioExecutor::new()).build_http(),
            tokens,
        }
    }

    async fn send(&self, method: Method, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        let mut req = Request::builder()
            .method(method.clone())
            .uri(format!("{}{path}", self.base_url))
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(authorization) = self.tokens.authorization("awzy-context")? {
            req = req.header(header::AUTHORIZATION, authorization);
        }
        let body = body.map(|b| Bytes::from(b.to_string())).unwrap_or_default();
        let res = tokio::time::timeout(TIMEOUT, self.http.request(req.body(Full::new(body))?))
            .await
            .with_context(|| format!("{method} {path} timed out"))??;
        let status = res.status();
        let bytes = res.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            bail!("{method} {path}: {status}: {}", String::from_utf8_lossy(&bytes));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Each principal's team, from the principals snapshot.
    pub async fn teams(&self) -> anyhow::Result<HashMap<Principal, String>> {
        Ok(teams(&self.send(Method::GET, "/v1/principals", None).await?))
    }

    /// Replaces these principals' scores.
    pub async fn put(&self, scores: &[Scored]) -> anyhow::Result<()> {
        for chunk in scores.chunks(BATCH) {
            let scores: Vec<Value> = chunk
                .iter()
                .map(|s| {
                    json!({
                        "subject": { "type": s.principal.0, "id": s.principal.1 },
                        "score": s.score.score,
                        "reasons": s.score.reasons,
                    })
                })
                .collect();
            self.send(Method::POST, "/v1/risk", Some(json!({ "scores": scores }))).await?;
        }
        Ok(())
    }
}

/// A principal's team is the smallest group it's directly in (ties by name), so a team is
/// "analysts" rather than "all-staff".
fn teams(snapshot: &Value) -> HashMap<Principal, String> {
    let entities: Vec<&Value> = snapshot["entities"].as_array().into_iter().flatten().collect();
    let groups_of = |e: &Value| -> Vec<String> {
        let parents = e["parents"].as_array().into_iter().flatten();
        parents.filter(|p| p["type"] == "Group").filter_map(|p| p["id"].as_str()).map(str::to_string).collect()
    };
    let mut size: HashMap<String, usize> = HashMap::new();
    for e in &entities {
        for g in groups_of(e) {
            *size.entry(g).or_default() += 1;
        }
    }
    let mut teams = HashMap::new();
    for e in &entities {
        let (Some(kind @ ("Human" | "ServiceAccount")), Some(id)) =
            (e["uid"]["type"].as_str(), e["uid"]["id"].as_str())
        else {
            continue;
        };
        if let Some(team) = groups_of(e).into_iter().min_by_key(|g| (size[g], g.clone())) {
            teams.insert((kind.to_string(), id.to_string()), team);
        }
    }
    teams
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_principals_team_is_the_smallest_group_its_in() {
        let member = |id: &str, groups: &[&str]| {
            let parents: Vec<Value> = groups.iter().map(|g| json!({ "type": "Group", "id": g })).collect();
            json!({ "uid": { "type": "Human", "id": id }, "attrs": {}, "parents": parents })
        };
        let snapshot = json!({ "version": 3, "entities": [
            member("ana", &["all-staff", "analysts"]),
            member("ben", &["all-staff", "analysts"]),
            member("cy", &["all-staff", "sales"]),
            member("dee", &["all-staff"]),
            { "uid": { "type": "ServiceAccount", "id": "genie" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Group", "id": "analysts" }, "attrs": {}, "parents": [{ "type": "Group", "id": "all-staff" }] },
        ]});
        let teams = teams(&snapshot);
        let team = |id: &str| teams.get(&("Human".to_string(), id.to_string())).map(String::as_str);
        assert_eq!(
            (team("ana"), team("ben"), team("cy"), team("dee")),
            (Some("analysts"), Some("analysts"), Some("sales"), Some("all-staff"))
        );
        assert_eq!(teams.len(), 4, "groups and principals in no group have no team");
    }
}
