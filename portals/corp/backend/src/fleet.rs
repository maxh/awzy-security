//! The install registry and heartbeat intake (ADR 0026).
//!
//! Staff register an install and get a reporting token, shown once. The install's reporter
//! sends a heartbeat every minute to `POST /v1/heartbeat` with that token. Only what the fixed
//! schema in `awzy_heartbeat` allows is accepted and stored.

use std::sync::Arc;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use awzy_heartbeat::Heartbeat;
use awzy_model::InstallId;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgRow, types::Json, PgPool, Row};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// No heartbeat for this long and an install shows as not reporting.
pub const REPORTING_TIMEOUT: Duration = Duration::minutes(5);

/// How long each heartbeat's numbers are kept for charts.
pub const SAMPLE_RETENTION: Duration = Duration::days(30);

/// Reporting tokens start with this, so a leaked one is recognizable in logs and scanners.
const TOKEN_PREFIX: &str = "awzyrt_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Stable,
    Fast,
    Pinned,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Fast => "fast",
            Self::Pinned => "pinned",
        }
    }

    fn parse(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "stable" => Ok(Self::Stable),
            "fast" => Ok(Self::Fast),
            "pinned" => Ok(Self::Pinned),
            other => Err(sqlx::Error::Decode(format!("unknown channel {other:?}").into())),
        }
    }
}

/// What staff enter to register an install.
#[derive(Debug, Clone)]
pub struct NewInstall {
    pub client: String,
    pub environment: String,
    pub cloud: String,
    pub region: String,
    pub channel: Channel,
    pub pinned_version: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("{0}")]
    Invalid(String),
    #[error("{client} already has an install called {environment}")]
    Exists { client: String, environment: String },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Lowercase letters, digits, and hyphens, 1 to 63 characters, like a DNS label.
fn is_slug(s: &str) -> bool {
    !s.is_empty() && s.len() <= 63 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl NewInstall {
    fn validate(&self) -> Result<(), RegisterError> {
        let invalid = |m: &str| Err(RegisterError::Invalid(m.to_string()));
        if !is_slug(&self.client) || !is_slug(&self.environment) || !is_slug(&self.region) {
            return invalid("client, environment, and region must be lowercase letters, digits, and hyphens");
        }
        if !["aws", "gcp", "azure"].contains(&self.cloud.as_str()) {
            return invalid("cloud must be aws, gcp, or azure");
        }
        match (self.channel, &self.pinned_version) {
            (Channel::Pinned, None) => invalid("a pinned install needs a pinned version"),
            (Channel::Stable | Channel::Fast, Some(_)) => invalid("only a pinned install has a pinned version"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallRecord {
    pub id: InstallId,
    pub client: String,
    pub environment: String,
    pub cloud: String,
    pub region: String,
    pub channel: Channel,
    pub pinned_version: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_heartbeat: Option<Heartbeat>,
    /// When staff retired it: its tokens stop working and it leaves the fleet views.
    pub retired_at: Option<DateTime<Utc>>,
    pub retired_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    /// A service isn't ready, or has fewer ready replicas than desired.
    Degraded,
    /// Reported before, but not within [`REPORTING_TIMEOUT`].
    NotReporting,
    NeverReported,
}

impl Health {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Degraded => "degraded",
            Self::NotReporting => "not_reporting",
            Self::NeverReported => "never_reported",
        }
    }

    fn parse(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "ok" => Ok(Self::Ok),
            "degraded" => Ok(Self::Degraded),
            "not_reporting" => Ok(Self::NotReporting),
            "never_reported" => Ok(Self::NeverReported),
            other => Err(sqlx::Error::Decode(format!("unknown health {other:?}").into())),
        }
    }
}

/// Where an install stands for alerting (ADR 0043).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlertState {
    /// The health it was last alerted on, or accepted without an alert.
    pub alerted: Option<Health>,
    /// A different health, and when it was first seen, that hasn't held long enough yet.
    pub held: Option<(Health, DateTime<Utc>)>,
}

impl InstallRecord {
    pub fn health(&self, now: DateTime<Utc>) -> Health {
        let (Some(seen), Some(hb)) = (self.last_seen_at, &self.last_heartbeat) else {
            return Health::NeverReported;
        };
        if now - seen > REPORTING_TIMEOUT {
            return Health::NotReporting;
        }
        if hb.services.iter().any(|s| !s.ready || s.replicas_ready < s.replicas_desired) {
            return Health::Degraded;
        }
        Health::Ok
    }

    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: InstallId::from_uuid(row.try_get("id")?),
            client: row.try_get("client")?,
            environment: row.try_get("environment")?,
            cloud: row.try_get("cloud")?,
            region: row.try_get("region")?,
            channel: Channel::parse(row.try_get("channel")?)?,
            pinned_version: row.try_get("pinned_version")?,
            created_by: row.try_get("created_by")?,
            created_at: row.try_get("created_at")?,
            last_seen_at: row.try_get("last_seen_at")?,
            last_heartbeat: row.try_get::<Option<Json<Heartbeat>>, _>("last_heartbeat")?.map(|j| j.0),
            retired_at: row.try_get("retired_at")?,
            retired_by: row.try_get("retired_by")?,
        })
    }
}

const COLUMNS: &str = "id, client, environment, cloud, region, channel, pinned_version, created_by, created_at, last_seen_at, last_heartbeat, retired_at, retired_by";

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// How far back a chart looks. Each window is bucketed so a chart has a few hundred points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    Day,
    Week,
    Month,
}

impl Window {
    /// (how far back, bucket width)
    fn span(self) -> (Duration, Duration) {
        match self {
            Self::Day => (Duration::days(1), Duration::minutes(5)),
            Self::Week => (Duration::days(7), Duration::minutes(30)),
            Self::Month => (Duration::days(30), Duration::hours(2)),
        }
    }
}

/// One bucket of heartbeats. Each figure is the worst the bucket saw, except throughput, which
/// is the average. A bucket with no heartbeats isn't returned, so gaps show as gaps.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Sample {
    pub at: DateTime<Utc>,
    pub decisions_per_second: Option<f64>,
    pub pdp_p99_ms: Option<f64>,
    pub error_rate: Option<f64>,
    pub kafka_lag: Option<i64>,
    pub clickhouse_disk_used_percent: Option<f64>,
    /// Some heartbeat in the bucket had a service that wasn't fully ready.
    pub degraded: bool,
}

#[derive(Clone)]
pub struct FleetStore {
    pool: PgPool,
    /// The registry proxy's pull tokens (ADR 0028), when this portal issues them.
    pull_tokens: Option<Arc<awzy_registry::tokens::TokenStore>>,
}

impl FleetStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, pull_tokens: None }
    }

    /// Issue and revoke installs' registry pull tokens along with their registration.
    pub fn with_pull_tokens(mut self, tokens: awzy_registry::tokens::TokenStore) -> Self {
        self.pull_tokens = Some(Arc::new(tokens));
        self
    }

    /// A new pull token for `id`, replacing any it had; None when this portal doesn't issue them.
    pub async fn issue_pull_token(&self, id: InstallId, by: &str) -> Result<Option<String>, sqlx::Error> {
        match &self.pull_tokens {
            Some(tokens) => tokens.create(id, by).await.map(Some),
            None => Ok(None),
        }
    }

    /// Retires an install: its reporting and pull tokens stop working, it leaves the fleet
    /// views, and its client and environment can be registered again. None if there's no
    /// active install with that ID.
    pub async fn retire(&self, id: InstallId, by: &str) -> Result<Option<InstallRecord>, sqlx::Error> {
        let row = sqlx::query(&format!(
            "UPDATE installs SET retired_at = now(), retired_by = $2 WHERE id = $1 AND retired_at IS NULL RETURNING {COLUMNS}"
        ))
        .bind(id.uuid())
        .bind(by)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        if let Some(tokens) = &self.pull_tokens {
            tokens.revoke(id).await?;
        }
        InstallRecord::from_row(&row).map(Some)
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    /// Registers an install. Returns it and its reporting token, which isn't stored.
    pub async fn register(&self, new: &NewInstall, created_by: &str) -> Result<(InstallRecord, String), RegisterError> {
        new.validate()?;
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(secret));
        let row = sqlx::query(&format!(
            "INSERT INTO installs (id, client, environment, cloud, region, channel, pinned_version, token_sha256, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING {COLUMNS}"
        ))
        .bind(InstallId::new().uuid())
        .bind(&new.client)
        .bind(&new.environment)
        .bind(&new.cloud)
        .bind(&new.region)
        .bind(new.channel.as_str())
        .bind(&new.pinned_version)
        .bind(token_hash(&token))
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                RegisterError::Exists { client: new.client.clone(), environment: new.environment.clone() }
            }
            e => RegisterError::Db(e),
        })?;
        Ok((InstallRecord::from_row(&row)?, token))
    }

    /// Ordered by registration. `after`: continue after this install.
    pub async fn list(&self, after: Option<InstallId>, limit: i64) -> Result<Vec<InstallRecord>, sqlx::Error> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM installs WHERE id > $1 AND retired_at IS NULL ORDER BY id LIMIT $2"
        ))
        .bind(after.map(|a| a.uuid()).unwrap_or_default())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(InstallRecord::from_row).collect()
    }

    pub async fn get(&self, id: InstallId) -> Result<Option<InstallRecord>, sqlx::Error> {
        sqlx::query(&format!("SELECT {COLUMNS} FROM installs WHERE id = $1"))
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(InstallRecord::from_row)
            .transpose()
    }

    /// The install a reporting token belongs to.
    pub async fn authenticate(&self, token: &str) -> Result<Option<InstallId>, sqlx::Error> {
        let id: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM installs WHERE token_sha256 = $1 AND retired_at IS NULL")
                .bind(token_hash(token))
                .fetch_optional(&self.pool)
                .await?;
        Ok(id.map(InstallId::from_uuid))
    }

    /// Stores a validated heartbeat as the install's latest, keeps its numbers for charts, and
    /// notes a new chart version.
    pub async fn record_heartbeat(&self, hb: &Heartbeat, received_at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE installs SET last_heartbeat = $2, last_seen_at = $3 WHERE id = $1")
            .bind(hb.install_id.uuid())
            .bind(Json(hb))
            .bind(received_at)
            .execute(&mut *tx)
            .await?;
        let m = &hb.metrics;
        let healthy = hb.services.iter().filter(|s| s.ready && s.replicas_ready >= s.replicas_desired).count();
        sqlx::query(
            "INSERT INTO heartbeat_samples (install_id, received_at, decisions_per_second, pdp_p99_ms, error_rate,
                                            kafka_lag, clickhouse_disk_used_percent, services_healthy, services_total)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT DO NOTHING",
        )
        .bind(hb.install_id.uuid())
        .bind(received_at)
        .bind(m.decisions_per_second)
        .bind(m.pdp_p99_ms)
        .bind(m.error_rate)
        .bind(m.kafka_lag.map(|l| i64::try_from(l).unwrap_or(i64::MAX)))
        .bind(m.clickhouse_disk_used_percent)
        .bind(healthy as i32)
        .bind(hb.services.len() as i32)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM heartbeat_samples WHERE install_id = $1 AND received_at < $2")
            .bind(hb.install_id.uuid())
            .bind(received_at - SAMPLE_RETENTION)
            .execute(&mut *tx)
            .await?;
        if let Some(version) = &hb.chart_version {
            sqlx::query(
                "INSERT INTO install_versions (install_id, chart_version, first_seen_at) VALUES ($1, $2, $3)
                 ON CONFLICT DO NOTHING",
            )
            .bind(hb.install_id.uuid())
            .bind(version)
            .bind(received_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// The install's heartbeats over `window` up to `now`, bucketed, oldest first.
    pub async fn samples(&self, id: InstallId, window: Window, now: DateTime<Utc>) -> Result<Vec<Sample>, sqlx::Error> {
        let (back, bucket) = window.span();
        sqlx::query_as(
            "SELECT date_bin(make_interval(secs => $3), received_at, TIMESTAMPTZ 'epoch') AS at,
                    avg(decisions_per_second) AS decisions_per_second,
                    max(pdp_p99_ms) AS pdp_p99_ms,
                    max(error_rate) AS error_rate,
                    max(kafka_lag) AS kafka_lag,
                    max(clickhouse_disk_used_percent) AS clickhouse_disk_used_percent,
                    bool_or(services_healthy < services_total) AS degraded
             FROM heartbeat_samples
             WHERE install_id = $1 AND received_at > $2
             GROUP BY 1 ORDER BY 1",
        )
        .bind(id.uuid())
        .bind(now - back)
        .bind(bucket.num_seconds() as f64)
        .fetch_all(&self.pool)
        .await
    }

    /// Every install that isn't retired.
    pub async fn all_installs(&self) -> Result<Vec<InstallRecord>, sqlx::Error> {
        let rows = sqlx::query(&format!("SELECT {COLUMNS} FROM installs WHERE retired_at IS NULL ORDER BY id"))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(InstallRecord::from_row).collect()
    }

    pub async fn alert_state(&self, id: InstallId) -> Result<AlertState, sqlx::Error> {
        let (alerted, held, since): (Option<String>, Option<String>, Option<DateTime<Utc>>) =
            sqlx::query_as("SELECT alerted_health, held_health, held_since FROM installs WHERE id = $1")
                .bind(id.uuid())
                .fetch_one(&self.pool)
                .await?;
        Ok(AlertState {
            alerted: alerted.as_deref().map(Health::parse).transpose()?,
            held: match (held, since) {
                (Some(h), Some(at)) => Some((Health::parse(&h)?, at)),
                _ => None,
            },
        })
    }

    /// Starts holding `health`, first seen at `at`.
    pub async fn hold_alert(&self, id: InstallId, health: Health, at: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE installs SET held_health = $2, held_since = $3 WHERE id = $1")
            .bind(id.uuid())
            .bind(health.as_str())
            .bind(at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Records `health` as known without alerting, and drops any held change.
    pub async fn accept_health(&self, id: InstallId, health: Health) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE installs SET alerted_health = $2, held_health = NULL, held_since = NULL WHERE id = $1")
            .bind(id.uuid())
            .bind(health.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Moves the install from `from` to `to` if nobody else has: true for the one caller whose
    /// update lands, which then sends the alert.
    pub async fn claim_alert(&self, id: InstallId, from: Option<Health>, to: Health) -> Result<bool, sqlx::Error> {
        let done = sqlx::query(
            "UPDATE installs SET alerted_health = $3, held_health = NULL, held_since = NULL
             WHERE id = $1 AND alerted_health IS NOT DISTINCT FROM $2",
        )
        .bind(id.uuid())
        .bind(from.map(Health::as_str))
        .bind(to.as_str())
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Undoes a claim whose alert couldn't be sent, so the next check tries again.
    pub async fn unclaim_alert(&self, id: InstallId, from: Option<Health>, to: Health) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE installs SET alerted_health = $2 WHERE id = $1 AND alerted_health = $3")
            .bind(id.uuid())
            .bind(from.map(Health::as_str))
            .bind(to.as_str())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Chart versions the install has reported, newest first.
    pub async fn versions(&self, id: InstallId) -> Result<Vec<(String, DateTime<Utc>)>, sqlx::Error> {
        sqlx::query_as("SELECT chart_version, first_seen_at FROM install_versions WHERE install_id = $1 ORDER BY first_seen_at DESC")
            .bind(id.uuid())
            .fetch_all(&self.pool)
            .await
    }
}

/// `POST /v1/heartbeat`, authenticated with the install's reporting token.
pub fn intake_routes(store: FleetStore) -> Router {
    Router::new().route("/v1/heartbeat", post(heartbeat)).layer(DefaultBodyLimit::max(64 * 1024)).with_state(store)
}

async fn heartbeat(State(store): State<FleetStore>, headers: HeaderMap, body: Bytes) -> Response {
    let token =
        headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
    let install = match token {
        Some(t) => store.authenticate(t).await,
        None => Ok(None),
    };
    let install = match install {
        Ok(Some(id)) => id,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "unknown reporting token").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "heartbeat: looking up the token failed");
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    let hb: Heartbeat = match serde_json::from_slice(&body) {
        Ok(hb) => hb,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("not a heartbeat: {e}")).into_response(),
    };
    if hb.install_id != install {
        return (StatusCode::FORBIDDEN, "the heartbeat names a different install than the token").into_response();
    }
    let now = Utc::now();
    if let Err(e) = hb.validate(now) {
        return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
    }
    match store.record_heartbeat(&hb, now).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error = %e, install = %install, "heartbeat: storing failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

/// Orders versions like `0.10.0` after `0.9.0`: runs of digits compare as numbers.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    fn chunks(s: &str) -> Vec<Result<u64, String>> {
        let mut out = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            let n = if digits > 0 { digits } else { rest.chars().take_while(|c| !c.is_ascii_digit()).count() };
            let (head, tail) = rest.split_at(n);
            out.push(head.parse::<u64>().map_err(|_| head.to_string()));
            rest = tail;
        }
        out
    }
    chunks(a).cmp(&chunks(b))
}

/// Integration tests against Postgres; skipped without AWZY_TEST_POSTGRES_URL.
#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    use super::*;

    pub(crate) async fn test_store() -> Option<FleetStore> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", Uuid::now_v7().simple());
        let admin = PgPool::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        let store = FleetStore::new(pool);
        store.migrate().await.unwrap();
        Some(store)
    }

    pub(crate) fn new_install(client: &str, environment: &str) -> NewInstall {
        NewInstall {
            client: client.into(),
            environment: environment.into(),
            cloud: "aws".into(),
            region: "us-east-2".into(),
            channel: Channel::Fast,
            pinned_version: None,
        }
    }

    #[test]
    fn versions_compare_by_number() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Greater);
        assert_eq!(compare_versions("0.5.0-rc.2", "0.5.0-rc.10"), Less);
        assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
    }

    #[test]
    fn registration_input_is_checked() {
        let mut bad = new_install("Acme Corp", "prod");
        assert!(bad.validate().is_err(), "client names are slugs");
        bad = new_install("acme", "prod");
        bad.channel = Channel::Pinned;
        assert!(bad.validate().is_err(), "pinned needs a version");
        bad.pinned_version = Some("0.4.1".into());
        assert!(bad.validate().is_ok());
        bad.cloud = "oracle".into();
        assert!(bad.validate().is_err());
    }

    #[tokio::test]
    async fn registers_once_per_client_environment_and_authenticates_tokens() {
        let Some(store) = test_store().await else { return };
        let (install, token) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
        assert!(token.starts_with("awzyrt_"));
        assert_eq!(store.authenticate(&token).await.unwrap(), Some(install.id));
        assert_eq!(store.authenticate("awzyrt_wrong").await.unwrap(), None);
        assert!(matches!(store.register(&new_install("acme", "prod"), "max").await, Err(RegisterError::Exists { .. })));
        assert_eq!(install.health(Utc::now()), Health::NeverReported);
    }

    #[tokio::test]
    async fn a_retired_install_stops_authenticating_and_frees_its_name() {
        let Some(store) = test_store().await else { return };
        let url = std::env::var("AWZY_TEST_POSTGRES_URL").unwrap();
        let registry_schema = format!("test_registry_{}", Uuid::now_v7().simple());
        let pull_tokens = awzy_registry::tokens::TokenStore::connect(&url, &registry_schema).await.unwrap();
        let store = store.with_pull_tokens(pull_tokens);

        let (old, reporting) = store.register(&new_install("acme", "demo"), "max").await.unwrap();
        let pull = store.issue_pull_token(old.id, "max").await.unwrap().expect("this store issues pull tokens");
        let registry = store.pull_tokens.clone().unwrap();
        assert!(registry.check(old.id, &pull).await.unwrap());

        let retired = store.retire(old.id, "max").await.unwrap().expect("it was active");
        assert_eq!(retired.retired_by.as_deref(), Some("max"));
        assert_eq!(store.authenticate(&reporting).await.unwrap(), None, "its reporting token stops working");
        assert!(!registry.check(old.id, &pull).await.unwrap(), "and so does its pull token");
        assert!(store.retire(old.id, "max").await.unwrap().is_none(), "retiring twice changes nothing");
        assert!(store.all_installs().await.unwrap().iter().all(|i| i.id != old.id), "it leaves the fleet");
        assert!(store.get(old.id).await.unwrap().is_some(), "but its record stays");

        let (new, _) = store.register(&new_install("acme", "demo"), "max").await.unwrap();
        assert_ne!(new.id, old.id, "the same client and environment register again, as a new install");
    }

    fn beat(install: InstallId, p99: f64, dps: f64, ready: u32) -> Heartbeat {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "install_qid": install, "sent_at": Utc::now(), "chart_version": "0.5.0",
            "services": [{ "service": "awzy-pdp", "version": "0.5.0", "image_digest": null, "replicas_desired": 2,
                           "replicas_ready": ready, "restarts_24h": 0, "ready": true }],
            "metrics": { "decisions_per_second": dps, "pdp_p99_ms": p99 },
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn chart_samples_show_the_worst_of_each_bucket_and_leave_gaps() {
        let Some(store) = test_store().await else { return };
        let (install, _) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
        let now: DateTime<Utc> = "2026-10-09T12:00:00Z".parse().unwrap();
        let at = |minutes_ago: i64| now - Duration::minutes(minutes_ago);
        // One 5-minute bucket with two heartbeats, an hour of silence, then one more.
        store.record_heartbeat(&beat(install.id, 2.0, 40.0, 2), at(64)).await.unwrap();
        store.record_heartbeat(&beat(install.id, 9.0, 60.0, 1), at(63)).await.unwrap();
        store.record_heartbeat(&beat(install.id, 1.5, 50.0, 2), at(2)).await.unwrap();

        let samples = store.samples(install.id, Window::Day, now).await.unwrap();
        assert_eq!(samples.len(), 2, "the silent hour has no buckets: {samples:?}");
        assert_eq!(samples[0].at, "2026-10-09T10:55:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(samples[0].pdp_p99_ms, Some(9.0), "worst p99 in the bucket");
        assert_eq!(samples[0].decisions_per_second, Some(50.0), "average throughput");
        assert!(samples[0].degraded, "one PDP replica wasn't ready");
        assert!(!samples[1].degraded);
    }

    #[tokio::test]
    async fn samples_older_than_the_retention_are_dropped() {
        let Some(store) = test_store().await else { return };
        let (install, _) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
        let now: DateTime<Utc> = "2026-10-09T12:00:00Z".parse().unwrap();
        store.record_heartbeat(&beat(install.id, 1.0, 1.0, 2), now - Duration::days(31)).await.unwrap();
        store.record_heartbeat(&beat(install.id, 1.0, 1.0, 2), now).await.unwrap();
        let kept: i64 =
            sqlx::query_scalar("SELECT count(*) FROM heartbeat_samples").fetch_one(&store.pool).await.unwrap();
        assert_eq!(kept, 1);
    }
}
