//! awzy-connectors' status API (`GET /v1/status`) as types, shared by awzy-connectors, which
//! serves it, and the client portal's Connectors page, which reads it (#221, #222).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a configured connector reports about itself. Never its credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorStatus {
    /// drive or jira.
    pub name: String,
    /// What it was configured with, by name, for display.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
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

/// `GET /v1/status`: every configured connector.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StatusList {
    pub connectors: Vec<ConnectorStatus>,
}
