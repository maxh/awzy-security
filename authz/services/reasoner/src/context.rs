//! awzy-context, for who is on which team, documents' text, and to write scores and
//! investigations, with this service's token for awzy-context (ADR 0055).

use std::collections::{BTreeSet, HashMap};

use awzy_context_api::{
    model::{CedarEntity, PrincipalsSnapshot, RiskScore},
    ContextApi, ContextError,
};
use serde_json::Value;

use crate::{activity::Principal, Scored};

/// The most scores one request carries; awzy-context takes up to 10,000.
const BATCH: usize = 1000;

/// awzy-context's API as the reasoner uses it, through its one client (#222).
pub struct ContextClient {
    api: ContextApi,
}

impl ContextClient {
    pub fn new(base_url: &str, tokens: awzy_service_auth::TokenSource) -> Self {
        Self { api: ContextApi::new(base_url, tokens) }
    }

    /// The principals snapshot the PDPs load: people, groups, and service accounts.
    pub async fn snapshot(&self) -> Result<PrincipalsSnapshot, ContextError> {
        self.api.principals().await
    }

    /// Each principal's team, from the principals snapshot.
    pub async fn teams(&self) -> Result<HashMap<Principal, String>, ContextError> {
        Ok(teams(&self.snapshot().await?))
    }

    /// A resource's title and text, or None when awzy-context doesn't have it.
    pub async fn resource(&self, id: &str) -> Result<Option<(String, String)>, ContextError> {
        Ok(self.api.resource(id).await?.map(|r| (r.name, r.text.unwrap_or_default())))
    }

    /// Records an investigation (ADR 0071) and returns its QID.
    pub async fn put_investigation(&self, record: Value) -> Result<String, ContextError> {
        Ok(self.api.put_investigation(&record).await?.qid)
    }

    /// Everyone investigated since `since`.
    pub async fn investigated_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<BTreeSet<Principal>, ContextError> {
        let list = self.api.investigations_since(since, 500).await?;
        Ok(list.into_iter().map(|i| (i.principal.kind, i.principal.id)).collect())
    }

    /// Tokens spent on investigations since `since`.
    pub async fn investigation_tokens(&self, since: chrono::DateTime<chrono::Utc>) -> Result<u64, ContextError> {
        let usage = self.api.investigation_usage(since).await?;
        Ok((usage.input_tokens.max(0) + usage.output_tokens.max(0)).unsigned_abs())
    }

    /// Replaces these principals' scores.
    pub async fn put(&self, scores: &[Scored]) -> Result<(), ContextError> {
        for chunk in scores.chunks(BATCH) {
            let scores = chunk
                .iter()
                .map(|s| RiskScore {
                    principal: awzy_context_api::model::EntityUid {
                        kind: s.principal.0.clone(),
                        id: s.principal.1.clone(),
                    },
                    score: s.score.score,
                    reasons: s.score.reasons.clone(),
                    automation: s.automation.as_ref().map(|a| a.score),
                    automation_reasons: s.automation.as_ref().map(|a| a.reasons.clone()).unwrap_or_default(),
                })
                .collect();
            self.api.put_risk(scores).await?;
        }
        Ok(())
    }
}

/// The groups an entity is directly in.
fn groups_of(e: &CedarEntity) -> Vec<String> {
    e.parents.iter().filter(|p| p.kind == "Group").map(|p| p.id.clone()).collect()
}

/// A person or service account's key, or None for any other entity.
fn principal_of(e: &CedarEntity) -> Option<Principal> {
    matches!(e.uid.kind.as_str(), "Human" | "ServiceAccount").then(|| (e.uid.kind.clone(), e.uid.id.clone()))
}

/// Every group each principal is in, directly or through the groups it's in.
pub fn memberships(snapshot: &PrincipalsSnapshot) -> HashMap<Principal, BTreeSet<String>> {
    let parents: HashMap<String, Vec<String>> =
        snapshot.entities.iter().filter(|e| e.uid.kind == "Group").map(|e| (e.uid.id.clone(), groups_of(e))).collect();
    let mut out = HashMap::new();
    for e in &snapshot.entities {
        let Some(principal) = principal_of(e) else { continue };
        let mut all = BTreeSet::new();
        let mut todo = groups_of(e);
        while let Some(g) = todo.pop() {
            if all.insert(g.clone()) {
                todo.extend(parents.get(&g).cloned().unwrap_or_default());
            }
        }
        out.insert(principal, all);
    }
    out
}

/// A principal's team is the smallest group it's directly in (ties by name), so a team is
/// "analysts" rather than "all-staff".
fn teams(snapshot: &PrincipalsSnapshot) -> HashMap<Principal, String> {
    let mut size: HashMap<String, usize> = HashMap::new();
    for e in &snapshot.entities {
        for g in groups_of(e) {
            *size.entry(g).or_default() += 1;
        }
    }
    let mut teams = HashMap::new();
    for e in &snapshot.entities {
        let Some(principal) = principal_of(e) else { continue };
        if let Some(team) = groups_of(e).into_iter().min_by_key(|g| (size[g], g.clone())) {
            teams.insert(principal, team);
        }
    }
    teams
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn snapshot(v: Value) -> PrincipalsSnapshot {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn memberships_include_the_groups_groups_are_in() {
        let snapshot_json = json!({ "version": 1, "entities": [
            { "uid": { "type": "Human", "id": "lee" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it-admin" }] },
            { "uid": { "type": "Group", "id": "it-admin" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it" }] },
            { "uid": { "type": "Group", "id": "it" }, "attrs": {}, "parents": [{ "type": "Group", "id": "everyone" }] },
            { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [] },
        ]});
        let m = memberships(&snapshot(snapshot_json));
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
        let snapshot_json = json!({ "version": 3, "entities": [
            member("ana", &["all-staff", "analysts"]),
            member("ben", &["all-staff", "analysts"]),
            member("cy", &["all-staff", "sales"]),
            member("dee", &["all-staff"]),
            { "uid": { "type": "ServiceAccount", "id": "genie" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Group", "id": "analysts" }, "attrs": {}, "parents": [{ "type": "Group", "id": "all-staff" }] },
        ]});
        let teams = teams(&snapshot(snapshot_json));
        let team = |id: &str| teams.get(&("Human".to_string(), id.to_string())).map(String::as_str);
        assert_eq!(
            (team("ana"), team("ben"), team("cy"), team("dee")),
            (Some("analysts"), Some("analysts"), Some("sales"), Some("all-staff"))
        );
        assert_eq!(teams.len(), 4, "groups and principals in no group have no team");
    }
}
