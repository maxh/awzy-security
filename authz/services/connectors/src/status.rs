//! What each configured connector reports about itself: its settings (never its credentials),
//! when it last synced, and what went wrong. Served at `/v1/status` for the client portal's
//! Connectors page, so an admin can see a connector is working without kubectl.

use std::{
    collections::BTreeMap,
    sync::{LazyLock, Mutex},
};

use axum::{routing::get, Json, Router};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ConnectorStatus {
    pub name: &'static str,
    /// What it was configured with, by name, for display. No secrets.
    pub settings: BTreeMap<&'static str, String>,
    pub interval_secs: u64,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_failure_at: Option<DateTime<Utc>>,
    /// Why the last failed round failed. Cleared by a success.
    pub last_error: Option<String>,
    pub failures: u64,
    /// Written to or deleted from awzy-context since it started.
    pub written: u64,
    pub deleted: u64,
}

static STATUS: LazyLock<Mutex<BTreeMap<&'static str, ConnectorStatus>>> = LazyLock::new(Default::default);

fn with(name: &'static str, f: impl FnOnce(&mut ConnectorStatus)) {
    if let Some(s) = STATUS.lock().unwrap_or_else(|e| e.into_inner()).get_mut(name) {
        f(s);
    }
}

/// A connector that's configured, before its first round.
pub fn register(name: &'static str, settings: BTreeMap<&'static str, String>, interval_secs: u64) {
    let status = ConnectorStatus {
        name,
        settings,
        interval_secs,
        last_success_at: None,
        last_failure_at: None,
        last_error: None,
        failures: 0,
        written: 0,
        deleted: 0,
    };
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).insert(name, status);
}

pub fn succeeded(name: &'static str, written: usize, deleted: usize) {
    with(name, |s| {
        s.last_success_at = Some(Utc::now());
        s.last_error = None;
        s.written += written as u64;
        s.deleted += deleted as u64;
    });
}

pub fn failed(name: &'static str, error: &anyhow::Error) {
    with(name, |s| {
        s.last_failure_at = Some(Utc::now());
        s.last_error = Some(format!("{error:#}"));
        s.failures += 1;
    });
}

pub fn all() -> Vec<ConnectorStatus> {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
}

/// `GET /v1/status`: every configured connector, by name.
pub fn router() -> Router {
    Router::new().route("/v1/status", get(|| async { Json(serde_json::json!({ "connectors": all() })) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_success_clears_the_last_error_and_adds_to_the_counts() {
        register("test-connector", BTreeMap::from([("site", "https://example.test".into())]), 300);
        failed("test-connector", &anyhow::anyhow!("401 from the source"));
        let s = all().into_iter().find(|s| s.name == "test-connector").unwrap();
        assert_eq!((s.failures, s.last_error.as_deref()), (1, Some("401 from the source")));
        succeeded("test-connector", 3, 1);
        succeeded("test-connector", 2, 0);
        let s = all().into_iter().find(|s| s.name == "test-connector").unwrap();
        assert_eq!((s.last_error, s.written, s.deleted, s.failures), (None, 5, 1, 1));
        assert!(s.last_success_at.is_some() && s.last_failure_at.is_some());
        succeeded("not-registered", 1, 1);
        assert!(all().iter().all(|s| s.name != "not-registered"), "only configured connectors report");
    }
}
