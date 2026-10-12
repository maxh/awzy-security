//! The heartbeat an install sends the corp portal (ADR 0026).
//!
//! This is the whole of what leaves a client install for awzy (ADR 0009): which awzy services
//! run, at which versions, whether they are ready, and a few aggregate numbers. It is a fixed,
//! versioned schema: unknown fields are rejected, services are a closed list, and every string
//! is checked against a narrow pattern, so nothing free-form (hostnames, user names, policy
//! text) can ride along. The reporter logs each heartbeat it sends, so the client can read it.

use awzy_model::{InstallId, SupportSessionId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

/// The most support sessions one heartbeat reports.
pub const MAX_SUPPORT_SESSIONS: usize = 100;

/// The heartbeat an install's reporter last tried to send, and whether the corp portal took it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastSend {
    pub heartbeat: Heartbeat,
    pub delivered: bool,
    pub error: Option<String>,
}

/// What awzy-reporter serves its install's client portal at `GET /v1/last-heartbeat`: where
/// heartbeats go, how often, and the last one exactly as sent (none before the first).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LastHeartbeat {
    pub corp_url: String,
    pub interval_secs: u64,
    pub last: Option<LastSend>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Heartbeat {
    /// Always [`SCHEMA_VERSION`] for this type.
    pub schema_version: u32,
    /// `install_qid` on the wire (ADR 0058). `install_id` is still read from reporters older than
    /// the rename, since installs upgrade after the corp portal.
    #[serde(rename = "install_qid", alias = "install_id")]
    pub install_id: InstallId,
    pub sent_at: DateTime<Utc>,
    /// The awzy Helm chart version the install runs, when the reporter knows it.
    pub chart_version: Option<String>,
    pub services: Vec<ServiceStatus>,
    #[serde(default)]
    pub metrics: Metrics,
    /// Where each support session the install knows stands (ADR 0095): its ID, its status, and
    /// until when. Nothing else of it: not its areas, its reason, or who decided.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub support_sessions: Vec<SupportSessionState>,
    /// Whether the install runs the release its channel names, as Argo CD sees it: so awzy sees a
    /// release that can't install anywhere, not only where someone looks. Absent when the reporter
    /// can't read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<ReleaseState>,
}

/// The release's state, in closed values only: nothing of Argo CD's messages leaves the install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseState {
    pub sync: ReleaseSync,
    pub health: ReleaseHealth,
    /// Why the release isn't moving, when Argo CD says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<ReleaseBlocker>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseSync {
    Synced,
    OutOfSync,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseHealth {
    Healthy,
    Progressing,
    Degraded,
    Suspended,
    Missing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseBlocker {
    /// The release needs a newer bootstrap than the install has (ADR 0098): the client applies it.
    BootstrapTooOld,
    /// Syncing it fails: an object the API server refuses, or a hook that fails. awzy's to fix.
    SyncError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportSessionState {
    #[serde(rename = "qid")]
    pub id: SupportSessionId,
    pub status: SupportStatus,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportStatus {
    Requested,
    Approved,
    Rejected,
    Ended,
    Expired,
}

impl SupportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Ended => "ended",
            Self::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Requested, Self::Approved, Self::Rejected, Self::Ended, Self::Expired]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

/// What the corp portal answers a heartbeat with: support sessions awzy staff asked for that the
/// install hasn't reported yet (ADR 0095). This way the corp portal never calls into an install.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HeartbeatAnswer {
    #[serde(default)]
    pub support_requests: Vec<SupportRequest>,
}

/// One support session awzy staff asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupportRequest {
    #[serde(rename = "qid")]
    pub id: SupportSessionId,
    /// The staff member's corp portal username.
    pub requested_by: String,
    /// Client portal areas, e.g. "decisions".
    pub areas: Vec<String>,
    pub reason: String,
    pub duration_minutes: u32,
    pub requested_at: DateTime<Utc>,
}

/// One deployed unit inside the install.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceStatus {
    pub service: Service,
    pub version: Option<String>,
    /// `sha256:<64 hex>`, when running from an image.
    pub image_digest: Option<String>,
    pub replicas_desired: u32,
    pub replicas_ready: u32,
    pub restarts_24h: Option<u32>,
    /// The service's own `/readyz`.
    pub ready: bool,
}

/// The services an install can report. A closed list, so a heartbeat can't name anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Service {
    #[serde(rename = "awzy-pdp")]
    Pdp,
    #[serde(rename = "awzy-policy")]
    Policy,
    #[serde(rename = "awzy-gateway")]
    Gateway,
    #[serde(rename = "awzy-client-portal")]
    ClientPortal,
    #[serde(rename = "awzy-context")]
    Context,
    #[serde(rename = "awzy-connectors")]
    Connectors,
    #[serde(rename = "awzy-intake")]
    Intake,
    #[serde(rename = "awzy-reasoner")]
    Reasoner,
    #[serde(rename = "awzy-challenge")]
    Challenge,
    #[serde(rename = "awzy-reporter")]
    Reporter,
    #[serde(rename = "clickhouse")]
    Clickhouse,
}

impl Service {
    pub const ALL: [Service; 11] = [
        Self::Pdp,
        Self::Policy,
        Self::Gateway,
        Self::ClientPortal,
        Self::Context,
        Self::Connectors,
        Self::Intake,
        Self::Reasoner,
        Self::Challenge,
        Self::Reporter,
        Self::Clickhouse,
    ];

    /// The name on the wire, e.g. `awzy-pdp`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdp => "awzy-pdp",
            Self::Policy => "awzy-policy",
            Self::Gateway => "awzy-gateway",
            Self::ClientPortal => "awzy-client-portal",
            Self::Context => "awzy-context",
            Self::Connectors => "awzy-connectors",
            Self::Intake => "awzy-intake",
            Self::Reasoner => "awzy-reasoner",
            Self::Challenge => "awzy-challenge",
            Self::Reporter => "awzy-reporter",
            Self::Clickhouse => "clickhouse",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// Aggregates over the last minute, all optional: a reporter sends what it can measure.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub decisions_per_second: Option<f64>,
    pub pdp_p99_ms: Option<f64>,
    /// 0 to 1, across the install's services.
    pub error_rate: Option<f64>,
    pub kafka_lag: Option<u64>,
    /// 0 to 100.
    pub clickhouse_disk_used_percent: Option<f64>,
    /// The freshness targets, as seconds since each was last current: the worst across PDPs
    /// (containment feed, policy and principals syncs) and across connectors (ADR 0068). An
    /// install with no awzy-challenge, awzy-context, or connector doesn't send that one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment_feed_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_sync_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principals_sync_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_sync_age_seconds: Option<f64>,
    /// Tool calls awzy-gateway made since the last heartbeat while it couldn't reach awzy-pdp
    /// (ADR 0030): replayed, forwarded in monitor, or denied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_pdp_unreachable: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum InvalidHeartbeat {
    #[error("schema_version {0} is not supported; expected {SCHEMA_VERSION}")]
    SchemaVersion(u32),
    #[error("{0}: {1:?} doesn't look like a version")]
    Version(&'static str, String),
    #[error("{0:?} is not an image digest (sha256:<64 hex>)")]
    Digest(String),
    #[error("{0} is reported twice")]
    DuplicateService(&'static str),
    #[error("{0} is out of range")]
    OutOfRange(&'static str),
    #[error("sent_at is more than 10 minutes away from now")]
    Clock,
    #[error("more than {0} support sessions")]
    TooManySupportSessions(usize),
}

/// A version string: up to 64 of `[0-9A-Za-z.+-]`, e.g. `0.5.0-rc.1+3f1a`.
pub fn is_version(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

fn is_digest(s: &str) -> bool {
    s.strip_prefix("sha256:")
        .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

impl Heartbeat {
    /// Everything serde can't check: string patterns, ranges, and duplicates.
    pub fn validate(&self, now: DateTime<Utc>) -> Result<(), InvalidHeartbeat> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(InvalidHeartbeat::SchemaVersion(self.schema_version));
        }
        if (self.sent_at - now).num_minutes().abs() > 10 {
            return Err(InvalidHeartbeat::Clock);
        }
        if let Some(v) = &self.chart_version {
            if !is_version(v) {
                return Err(InvalidHeartbeat::Version("chart_version", v.clone()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for s in &self.services {
            if !seen.insert(s.service) {
                return Err(InvalidHeartbeat::DuplicateService(s.service.as_str()));
            }
            if let Some(v) = &s.version {
                if !is_version(v) {
                    return Err(InvalidHeartbeat::Version(s.service.as_str(), v.clone()));
                }
            }
            if let Some(d) = &s.image_digest {
                if !is_digest(d) {
                    return Err(InvalidHeartbeat::Digest(d.clone()));
                }
            }
            if s.replicas_desired > 1000 || s.replicas_ready > 1000 {
                return Err(InvalidHeartbeat::OutOfRange("replicas"));
            }
        }
        let m = &self.metrics;
        let in_range = |v: Option<f64>, max: f64| v.is_none_or(|v| v.is_finite() && (0.0..=max).contains(&v));
        if !in_range(m.decisions_per_second, 1e7) || !in_range(m.pdp_p99_ms, 1e6) {
            return Err(InvalidHeartbeat::OutOfRange("metrics"));
        }
        if !in_range(m.error_rate, 1.0) {
            return Err(InvalidHeartbeat::OutOfRange("error_rate"));
        }
        if !in_range(m.clickhouse_disk_used_percent, 100.0) {
            return Err(InvalidHeartbeat::OutOfRange("clickhouse_disk_used_percent"));
        }
        let ages = [
            m.containment_feed_age_seconds,
            m.policy_sync_age_seconds,
            m.principals_sync_age_seconds,
            m.connector_sync_age_seconds,
        ];
        // A year: anything older is as stale as it gets.
        if !ages.iter().all(|a| in_range(*a, 3.2e7)) {
            return Err(InvalidHeartbeat::OutOfRange("freshness"));
        }
        if self.support_sessions.len() > MAX_SUPPORT_SESSIONS {
            return Err(InvalidHeartbeat::TooManySupportSessions(MAX_SUPPORT_SESSIONS));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn sample() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "install_qid": InstallId::new().to_string(),
            "sent_at": Utc::now(),
            "chart_version": "0.5.0-rc.1",
            "services": [
                { "service": "awzy-pdp", "version": "0.5.0", "image_digest": format!("sha256:{}", "a".repeat(64)),
                  "replicas_desired": 3, "replicas_ready": 3, "restarts_24h": 0, "ready": true },
            ],
            "metrics": { "decisions_per_second": 12.5, "pdp_p99_ms": 1.2 },
        })
    }

    fn parse(v: serde_json::Value) -> Result<Heartbeat, String> {
        let h: Heartbeat = serde_json::from_value(v).map_err(|e| e.to_string())?;
        h.validate(Utc::now()).map_err(|e| e.to_string())?;
        Ok(h)
    }

    #[test]
    fn the_release_is_closed_values_and_reporters_without_it_are_still_read() {
        assert_eq!(parse(sample()).unwrap().release, None);
        let mut v = sample();
        v["release"] = json!({ "sync": "out_of_sync", "health": "degraded", "blocked_by": "sync_error" });
        let h = parse(v.clone()).unwrap();
        assert_eq!(
            h.release,
            Some(ReleaseState {
                sync: ReleaseSync::OutOfSync,
                health: ReleaseHealth::Degraded,
                blocked_by: Some(ReleaseBlocker::SyncError)
            })
        );
        assert_eq!(serde_json::to_value(&h).unwrap()["release"], v["release"]);
        v["release"]["message"] = json!("ValidatingAdmissionPolicy is invalid");
        assert!(parse(v.clone()).is_err(), "no free text");
        v["release"] = json!({ "sync": "half_synced", "health": "healthy" });
        assert!(parse(v).is_err(), "only the known states");
    }

    #[test]
    fn reporters_from_before_the_qid_rename_are_still_read() {
        let mut old = sample();
        let qid = old.as_object_mut().unwrap().remove("install_qid").unwrap();
        old["install_id"] = qid.clone();
        assert_eq!(parse(old).unwrap().install_id.to_string(), qid.as_str().unwrap());
        let sent = serde_json::to_value(parse(sample()).unwrap()).unwrap();
        assert!(sent.get("install_qid").is_some() && sent.get("install_id").is_none(), "sends the new name");
    }

    #[test]
    fn accepts_the_fixed_schema() {
        let h = parse(sample()).unwrap();
        assert_eq!(h.services[0].service, Service::Pdp);
    }

    #[test]
    fn rejects_anything_free_form() {
        let mut extra = sample();
        extra["hostname"] = json!("db01.acme.internal");
        assert!(parse(extra).unwrap_err().contains("unknown field"));

        let mut label = sample();
        label["services"][0]["labels"] = json!({ "team": "payments" });
        assert!(parse(label).unwrap_err().contains("unknown field"));

        let mut unknown = sample();
        unknown["services"][0]["service"] = json!("acme-billing");
        assert!(parse(unknown).unwrap_err().contains("unknown variant"));

        let mut sneaky = sample();
        sneaky["chart_version"] = json!("0.5.0 alice@acme.com");
        assert!(parse(sneaky).unwrap_err().contains("doesn't look like a version"));

        let mut digest = sample();
        digest["services"][0]["image_digest"] = json!("latest");
        assert!(parse(digest).is_err());

        let mut twice = sample();
        let pdp = twice["services"][0].clone();
        twice["services"].as_array_mut().unwrap().push(pdp);
        assert!(parse(twice).unwrap_err().contains("reported twice"));

        let mut rate = sample();
        rate["metrics"]["error_rate"] = json!(3.0);
        assert!(parse(rate).is_err());

        let mut age = sample();
        age["metrics"]["policy_sync_age_seconds"] = json!(-1.0);
        assert!(parse(age).unwrap_err().contains("freshness"));
    }

    #[test]
    fn freshness_is_optional_and_left_out_when_unknown() {
        let mut fresh = sample();
        fresh["metrics"]["containment_feed_age_seconds"] = json!(0.4);
        fresh["metrics"]["gateway_pdp_unreachable"] = json!(0);
        let h = parse(fresh).unwrap();
        assert_eq!(h.metrics.containment_feed_age_seconds, Some(0.4));
        let sent = serde_json::to_value(parse(sample()).unwrap()).unwrap();
        assert!(
            sent["metrics"].get("connector_sync_age_seconds").is_none(),
            "older corp portals reject fields they don't know"
        );
    }

    #[test]
    fn a_heartbeat_reports_support_sessions_by_id_status_and_expiry_only() {
        let mut hb = sample();
        hb["support_sessions"] =
            json!([{ "qid": SupportSessionId::new().to_string(), "status": "approved", "expires_at": Utc::now() }]);
        let parsed: Heartbeat = serde_json::from_value(hb.clone()).unwrap();
        assert_eq!(parsed.support_sessions[0].status, SupportStatus::Approved);
        parsed.validate(Utc::now()).unwrap();
        // Nothing more of a session can ride along.
        hb["support_sessions"][0]["reason"] = json!("customer data here");
        assert!(serde_json::from_value::<Heartbeat>(hb).is_err());
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            /// A version is exactly 1 to 64 of `[0-9A-Za-z.+-]`, so nothing else reaches the
            /// corp portal's pages as one.
            #[test]
            fn a_version_is_its_alphabet_and_length(s in "[0-9A-Za-z._+-]{1,64}", other in ".*") {
                prop_assert!(is_version(&s));
                let fits = !other.is_empty()
                    && other.len() <= 64
                    && other.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b));
                prop_assert_eq!(is_version(&other), fits);
            }

            /// Whatever a reporter sends, reading it answers rather than panicking.
            #[test]
            fn reading_any_text_is_total(text in ".*") {
                let _ = serde_json::from_str::<Heartbeat>(&text).map(|h| h.validate(Utc::now()));
            }
        }
    }
}
