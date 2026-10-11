//! awzy-context, for who is on which team, documents' text, and to write scores and
//! investigations, with this service's token for awzy-context (ADR 0055).

use std::{
    collections::{BTreeSet, HashMap},
    time::Duration,
};

use anyhow::{bail, Context};
use axum::http::{header, Method, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde_json::{json, Value};

use crate::{activity::Principal, Scored};

/// The most scores one request carries; awzy-context takes up to 10,000.
const BATCH: usize = 1000;
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct ContextClient {
    base_url: String,
    http: awzy_http::Client,
    tokens: awzy_service_auth::TokenSource,
}

impl ContextClient {
    pub fn new(base_url: &str, tokens: awzy_service_auth::TokenSource) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), http: awzy_http::client(), tokens }
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

    /// The principals snapshot the PDPs load: people, groups, and service accounts.
    pub async fn snapshot(&self) -> anyhow::Result<Value> {
        self.send(Method::GET, "/v1/principals", None).await
    }

    /// Each principal's team, from the principals snapshot.
    pub async fn teams(&self) -> anyhow::Result<HashMap<Principal, String>> {
        Ok(teams(&self.snapshot().await?))
    }

    /// A resource's title and text, or None when awzy-context doesn't have it.
    pub async fn resource(&self, id: &str) -> anyhow::Result<Option<(String, String)>> {
        match self.send(Method::GET, &format!("/v1/resources/{}", encode(id)), None).await {
            Ok(body) => {
                let r = &body["resource"];
                Ok(Some((r["name"].as_str().unwrap_or(id).to_string(), r["text"].as_str().unwrap_or("").to_string())))
            }
            Err(e) if e.to_string().contains(": 404 ") => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Records an investigation (ADR 0071) and returns its QID.
    pub async fn put_investigation(&self, record: Value) -> anyhow::Result<String> {
        let body = self.send(Method::POST, "/v1/investigations", Some(record)).await?;
        Ok(body["investigation"]["qid"].as_str().unwrap_or_default().to_string())
    }

    /// Everyone investigated since `since`.
    pub async fn investigated_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<BTreeSet<Principal>> {
        let path = format!("/v1/investigations?limit=500&since={}", encode(&since.to_rfc3339()));
        let body = self.send(Method::GET, &path, None).await?;
        Ok(body["investigations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| {
                Some((i["principal"]["type"].as_str()?.to_string(), i["principal"]["id"].as_str()?.to_string()))
            })
            .collect())
    }

    /// Tokens spent on investigations since `since`.
    pub async fn investigation_tokens(&self, since: chrono::DateTime<chrono::Utc>) -> anyhow::Result<u64> {
        let path = format!("/v1/investigations/usage?since={}", encode(&since.to_rfc3339()));
        let usage = &self.send(Method::GET, &path, None).await?["usage"];
        Ok(usage["input_tokens"].as_u64().unwrap_or(0) + usage["output_tokens"].as_u64().unwrap_or(0))
    }

    /// Replaces these principals' scores.
    pub async fn put(&self, scores: &[Scored]) -> anyhow::Result<()> {
        for chunk in scores.chunks(BATCH) {
            let scores: Vec<Value> = chunk
                .iter()
                .map(|s| {
                    let mut score = json!({
                        "principal": { "type": s.principal.0, "id": s.principal.1 },
                        "score": s.score.score,
                        "reasons": s.score.reasons,
                    });
                    if let Some(a) = &s.automation {
                        score["automation"] = json!(a.score);
                        score["automation_reasons"] = json!(a.reasons);
                    }
                    score
                })
                .collect();
            self.send(Method::POST, "/v1/risk", Some(json!({ "scores": scores }))).await?;
        }
        Ok(())
    }
}

/// Percent-encodes a path segment or query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Every group each principal is in, directly or through the groups it's in.
pub fn memberships(snapshot: &Value) -> HashMap<Principal, BTreeSet<String>> {
    let entities: Vec<&Value> = snapshot["entities"].as_array().into_iter().flatten().collect();
    let groups_of = |e: &Value| -> Vec<String> {
        let parents = e["parents"].as_array().into_iter().flatten();
        parents.filter(|p| p["type"] == "Group").filter_map(|p| p["id"].as_str()).map(str::to_string).collect()
    };
    let parents: HashMap<String, Vec<String>> = entities
        .iter()
        .filter(|e| e["uid"]["type"] == "Group")
        .filter_map(|e| Some((e["uid"]["id"].as_str()?.to_string(), groups_of(e))))
        .collect();
    let mut out = HashMap::new();
    for e in &entities {
        let (Some(kind @ ("Human" | "ServiceAccount")), Some(id)) =
            (e["uid"]["type"].as_str(), e["uid"]["id"].as_str())
        else {
            continue;
        };
        let mut all = BTreeSet::new();
        let mut todo = groups_of(e);
        while let Some(g) = todo.pop() {
            if all.insert(g.clone()) {
                todo.extend(parents.get(&g).cloned().unwrap_or_default());
            }
        }
        out.insert((kind.to_string(), id.to_string()), all);
    }
    out
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
    fn memberships_include_the_groups_groups_are_in() {
        let snapshot = json!({ "version": 1, "entities": [
            { "uid": { "type": "Human", "id": "lee" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it-admin" }] },
            { "uid": { "type": "Group", "id": "it-admin" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it" }] },
            { "uid": { "type": "Group", "id": "it" }, "attrs": {}, "parents": [{ "type": "Group", "id": "everyone" }] },
            { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [] },
        ]});
        let m = memberships(&snapshot);
        let lee: Vec<&str> = m[&("Human".to_string(), "lee".to_string())].iter().map(String::as_str).collect();
        assert_eq!(lee, vec!["everyone", "it", "it-admin"]);
        assert!(m[&("Human".to_string(), "nora".to_string())].is_empty());
    }

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
