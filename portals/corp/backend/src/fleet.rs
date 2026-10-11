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

/// Its migrations, tolerating ones a newer version already applied (ADR 0072): an older version
/// restarted mid-rollout, or rolled back to, still starts against the schema the newer one
/// expanded.
fn migrator() -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
}

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

    /// `stable`, `fast`, or `pinned`.
    pub fn from_name(s: &str) -> Option<Self> {
        Self::parse(s).ok()
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

/// The chart version a release channel names, and who moved it there.
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelVersion {
    pub channel: Channel,
    pub chart_version: String,
    pub changed_by: String,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum SetChannelError {
    #[error("a pinned install names its own version; only stable and fast are channels")]
    Pinned,
    #[error("{0:?} isn't a chart version: CI publishes 0.1.0-g<12 hex digits>")]
    Version(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// A version CI publishes: `0.<minor>.<patch>-g<first 12 hex digits of the commit>`.
pub fn is_chart_version(v: &str) -> bool {
    let Some((release, sha)) = v.split_once("-g") else { return false };
    let parts: Vec<&str> = release.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && sha.len() == 12
        && sha.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
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
        check_release(self.channel, self.pinned_version.as_deref())
    }
}

/// A pinned install names a chart version; one on stable or fast follows its channel.
fn check_release(channel: Channel, pinned_version: Option<&str>) -> Result<(), RegisterError> {
    let invalid = |m: String| Err(RegisterError::Invalid(m));
    match (channel, pinned_version) {
        (Channel::Pinned, None) => invalid("a pinned install needs a pinned version".into()),
        (Channel::Pinned, Some(v)) if !is_chart_version(v) => invalid(format!("{v:?} isn't a chart version")),
        (Channel::Stable | Channel::Fast, Some(_)) => invalid("only a pinned install has a pinned version".into()),
        _ => Ok(()),
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
    pub containment_feed_age_seconds: Option<f64>,
    pub policy_sync_age_seconds: Option<f64>,
    pub principals_sync_age_seconds: Option<f64>,
    pub connector_sync_age_seconds: Option<f64>,
    /// Summed over the bucket: every outage call counts.
    pub gateway_pdp_unreachable: Option<i64>,
    /// Some heartbeat in the bucket had a service that wasn't fully ready.
    pub degraded: bool,
}

#[derive(Clone)]
pub struct FleetStore {
    pool: PgPool,
    /// The registry proxy's pull tokens (ADR 0028), when this portal issues them.
    pull_tokens: Option<Arc<awzy_registry::tokens::TokenStore>>,
    /// Signs support session tokens (ADR 0095), when this portal has the key.
    pub(crate) support_signer: Option<Arc<crate::support::SupportSigner>>,
}

impl FleetStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool, pull_tokens: None, support_signer: None }
    }

    /// Signs support session tokens with this key (ADR 0095).
    pub fn with_support_signer(mut self, signer: crate::support::SupportSigner) -> Self {
        self.support_signer = Some(Arc::new(signer));
        self
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
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
    /// Each channel's current chart version, stable first. A channel never moved isn't listed.
    pub async fn channel_versions(&self) -> Result<Vec<ChannelVersion>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (channel) channel, chart_version, changed_by, changed_at FROM release_channel_changes
             ORDER BY channel DESC, changed_at DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(ChannelVersion {
                    channel: Channel::parse(row.try_get("channel")?)?,
                    chart_version: row.try_get("chart_version")?,
                    changed_by: row.try_get("changed_by")?,
                    changed_at: row.try_get("changed_at")?,
                })
            })
            .collect()
    }

    /// Points `channel` (stable or fast) at `chart_version` (ADR 0094). Every install on that
    /// channel moves to it at its next sync.
    pub async fn set_channel(
        &self,
        channel: Channel,
        chart_version: &str,
        by: &str,
    ) -> Result<ChannelVersion, SetChannelError> {
        if channel == Channel::Pinned {
            return Err(SetChannelError::Pinned);
        }
        if !is_chart_version(chart_version) {
            return Err(SetChannelError::Version(chart_version.to_string()));
        }
        let changed_at: DateTime<Utc> = sqlx::query_scalar(
            "INSERT INTO release_channel_changes (id, channel, chart_version, changed_by) VALUES ($1, $2, $3, $4)
             RETURNING changed_at",
        )
        .bind(Uuid::now_v7())
        .bind(channel.as_str())
        .bind(chart_version)
        .bind(by)
        .fetch_one(&self.pool)
        .await?;
        Ok(ChannelVersion { channel, chart_version: chart_version.to_string(), changed_by: by.to_string(), changed_at })
    }

    /// Moves an active install to another channel, or pins it to a chart version. It upgrades or
    /// rolls back the next time it pulls through the registry.
    pub async fn set_release(
        &self,
        id: InstallId,
        channel: Channel,
        pinned_version: Option<&str>,
    ) -> Result<Option<InstallRecord>, RegisterError> {
        check_release(channel, pinned_version)?;
        let row = sqlx::query(&format!(
            "UPDATE installs SET channel = $2, pinned_version = $3 WHERE id = $1 AND retired_at IS NULL
             RETURNING {COLUMNS}"
        ))
        .bind(id.uuid())
        .bind(channel.as_str())
        .bind(pinned_version)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| InstallRecord::from_row(&r)).transpose().map_err(RegisterError::Db)
    }

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
        migrator().run(&self.pool).await
    }

    /// At startup: apply the migrations, or with `AWZY_POSTGRES_MIGRATE=check` (deployed, where
    /// the migrate task applied them) only check they're applied (ADR 0087).
    pub async fn migrate_on_start(&self) -> anyhow::Result<()> {
        awzy_migrations::on_start(&migrator(), &self.pool).await
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
                                            kafka_lag, clickhouse_disk_used_percent, services_healthy, services_total,
                                            containment_feed_age_seconds, policy_sync_age_seconds,
                                            principals_sync_age_seconds, connector_sync_age_seconds,
                                            gateway_pdp_unreachable)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) ON CONFLICT DO NOTHING",
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
        .bind(m.containment_feed_age_seconds)
        .bind(m.policy_sync_age_seconds)
        .bind(m.principals_sync_age_seconds)
        .bind(m.connector_sync_age_seconds)
        .bind(m.gateway_pdp_unreachable.map(|n| i64::try_from(n).unwrap_or(i64::MAX)))
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
                    max(containment_feed_age_seconds) AS containment_feed_age_seconds,
                    max(policy_sync_age_seconds) AS policy_sync_age_seconds,
                    max(principals_sync_age_seconds) AS principals_sync_age_seconds,
                    max(connector_sync_age_seconds) AS connector_sync_age_seconds,
                    sum(gateway_pdp_unreachable)::BIGINT AS gateway_pdp_unreachable,
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
    if let Err(e) = store.record_heartbeat(&hb, now).await {
        tracing::error!(error = %e, install = %install, "heartbeat: storing failed");
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    // Support sessions (ADR 0095): where the install says each stands, and the requests it hasn't
    // got yet, in the answer, since the corp portal never calls into an install.
    if let Err(e) = store.record_support_states(install, &hb.support_sessions).await {
        tracing::error!(error = %e, install = %install, "heartbeat: storing support session states failed");
    }
    match store.pending_support(install).await {
        Ok(requests) if requests.is_empty() => StatusCode::NO_CONTENT.into_response(),
        Ok(requests) => axum::Json(awzy_heartbeat::HeartbeatAnswer { support_requests: requests }).into_response(),
        Err(e) => {
            tracing::error!(error = %e, install = %install, "heartbeat: reading support requests failed");
            StatusCode::NO_CONTENT.into_response()
        }
    }
}

/// Orders versions like `0.10.0` after `0.9.0`: runs of digits compare as numbers.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    /// A run of digits or of anything else; a number sorts before text at the same place.
    #[derive(PartialEq, Eq, PartialOrd, Ord)]
    enum Chunk<'a> {
        Number(u64),
        Text(&'a str),
    }
    fn chunks(s: &str) -> Vec<Chunk<'_>> {
        let mut out = Vec::new();
        let mut rest = s;
        while !rest.is_empty() {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            let n = if digits > 0 { digits } else { rest.chars().take_while(|c| !c.is_ascii_digit()).count() };
            let (head, tail) = rest.split_at(n);
            out.push(head.parse::<u64>().map_or(Chunk::Text(head), Chunk::Number));
            rest = tail;
        }
        out
    }
    chunks(a).cmp(&chunks(b))
}

/// Integration tests against Postgres; skipped without AWZY_TEST_POSTGRES_URL.
#[cfg(test)]
pub(crate) mod tests;
