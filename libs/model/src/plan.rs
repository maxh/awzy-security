//! A query plan's record (ADR 0113): what was asked, what the plan said, and what it rested on.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EnforcementMode, PlanId};

/// One row of the plan log. Serialized as JSON onto the plans topic and read by ClickHouse's
/// Kafka engine, so field names here are the ClickHouse column names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanRecord {
    pub id: PlanId,
    /// On the wire: `at_ms`, milliseconds since the Unix epoch.
    #[serde(rename = "at_ms", with = "chrono::serde::ts_milliseconds")]
    pub at: DateTime<Utc>,
    pub principal_type: String,
    pub principal_id: String,
    /// The person a service account acted for; empty when not delegated.
    pub on_behalf_of_type: String,
    pub on_behalf_of_id: String,
    pub action: String,
    pub resource_type: String,
    /// always_allowed | always_denied | challenge | conditional.
    pub outcome: String,
    pub mode: EnforcementMode,
    /// The challenge kinds when outcome is challenge, comma-separated; otherwise empty.
    pub challenge: String,
    /// The filter as JSON text, and a hash of it, which plans with the same filter share.
    pub filter: String,
    pub filter_hash: String,
    pub policies: Vec<String>,
    pub policy_version: i64,
    pub principals_version: i64,
    /// The request context as JSON text.
    pub context: String,
}
