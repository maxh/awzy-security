//! GraphQL for the fleet: installs, the deployed units inside each, and releases (ADR 0026).

use std::collections::BTreeMap;

use async_graphql::{
    connection::{query, Connection, Edge},
    Context, Enum, InputObject, Interface, Object, Result, SimpleObject, ID,
};
use awzy_heartbeat::{Service as WireService, ServiceStatus};
use awzy_model::InstallId;
use awzy_portal::{Actor, UtcTimestamp};
use chrono::Utc;

use crate::{
    auth::Staff,
    fleet::{self, FleetStore, InstallRecord, NewInstall, RegisterError},
};

const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 200;

fn store<'a>(ctx: &Context<'a>) -> Result<&'a FleetStore> {
    ctx.data::<FleetStore>().map_err(|_| "Postgres is not configured (AWZY_POSTGRES_URL)".into())
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum InstallHealth {
    Ok,
    /// A service isn't ready, or has fewer ready replicas than desired.
    Degraded,
    /// No heartbeat for 5 minutes.
    NotReporting,
    NeverReported,
}

impl From<fleet::Health> for InstallHealth {
    fn from(h: fleet::Health) -> Self {
        match h {
            fleet::Health::Ok => Self::Ok,
            fleet::Health::Degraded => Self::Degraded,
            fleet::Health::NotReporting => Self::NotReporting,
            fleet::Health::NeverReported => Self::NeverReported,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ReleaseChannel {
    Stable,
    Fast,
    Pinned,
}

impl From<fleet::Channel> for ReleaseChannel {
    fn from(c: fleet::Channel) -> Self {
        match c {
            fleet::Channel::Stable => Self::Stable,
            fleet::Channel::Fast => Self::Fast,
            fleet::Channel::Pinned => Self::Pinned,
        }
    }
}

impl From<ReleaseChannel> for fleet::Channel {
    fn from(c: ReleaseChannel) -> Self {
        match c {
            ReleaseChannel::Stable => Self::Stable,
            ReleaseChannel::Fast => Self::Fast,
            ReleaseChannel::Pinned => Self::Pinned,
        }
    }
}

/// How far back the Install page's charts look.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum MetricWindow {
    /// 5-minute buckets.
    Day,
    /// 30-minute buckets.
    Week,
    /// 2-hour buckets.
    Month,
}

impl From<MetricWindow> for fleet::Window {
    fn from(w: MetricWindow) -> Self {
        match w {
            MetricWindow::Day => Self::Day,
            MetricWindow::Week => Self::Week,
            MetricWindow::Month => Self::Month,
        }
    }
}

/// One bucket of heartbeats: the worst each figure got in it, and average throughput.
#[derive(SimpleObject)]
pub struct MetricSample {
    /// The bucket's start.
    pub at: UtcTimestamp,
    pub decisions_per_second: Option<f64>,
    pub pdp_p99_ms: Option<f64>,
    /// 0 to 1.
    pub error_rate: Option<f64>,
    pub kafka_lag: Option<i64>,
    /// 0 to 100.
    pub clickhouse_disk_used_percent: Option<f64>,
    /// Seconds since the worst PDP's containment feed was heard from (target: 2 s).
    pub containment_feed_age_seconds: Option<f64>,
    /// Seconds since the worst PDP synced policy (target: a published bundle live within 60 s).
    pub policy_sync_age_seconds: Option<f64>,
    /// Seconds since the worst PDP synced principals (target: 60 s).
    pub principals_sync_age_seconds: Option<f64>,
    /// Seconds since the stalest connector synced.
    pub connector_sync_age_seconds: Option<f64>,
    /// Tool calls awzy-gateway made without its PDP in the bucket.
    pub gateway_pdp_unreachable: Option<i64>,
    /// Some heartbeat in the bucket had a service that wasn't fully ready.
    pub degraded: bool,
}

/// One client install, as registered and as it last reported.
pub struct Install(pub InstallRecord);

#[Object]
impl Install {
    pub async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn client(&self) -> &str {
        &self.0.client
    }

    /// e.g. prod or staging.
    async fn environment(&self) -> &str {
        &self.0.environment
    }

    async fn cloud(&self) -> &str {
        &self.0.cloud
    }

    async fn region(&self) -> &str {
        &self.0.region
    }

    async fn channel(&self) -> ReleaseChannel {
        self.0.channel.into()
    }

    async fn pinned_version(&self) -> Option<&str> {
        self.0.pinned_version.as_deref()
    }

    async fn health(&self) -> InstallHealth {
        self.0.health(Utc::now()).into()
    }

    async fn last_seen_at(&self) -> Option<UtcTimestamp> {
        self.0.last_seen_at.map(UtcTimestamp)
    }

    /// The Helm chart version the install last reported.
    async fn chart_version(&self) -> Option<&str> {
        self.0.last_heartbeat.as_ref()?.chart_version.as_deref()
    }

    /// The deployed units inside the install, as last reported.
    async fn services(&self) -> Vec<DeployedService> {
        let mut services: Vec<_> =
            self.0.last_heartbeat.iter().flat_map(|h| h.services.iter().cloned()).map(DeployedService).collect();
        services.sort_by_key(|s| s.0.service);
        services
    }

    async fn metrics(&self) -> Option<InstallMetrics> {
        let m = &self.0.last_heartbeat.as_ref()?.metrics;
        Some(InstallMetrics {
            decisions_per_second: m.decisions_per_second,
            pdp_p99_ms: m.pdp_p99_ms,
            error_rate: m.error_rate,
            kafka_lag: m.kafka_lag,
            clickhouse_disk_used_percent: m.clickhouse_disk_used_percent,
            containment_feed_age_seconds: m.containment_feed_age_seconds,
            policy_sync_age_seconds: m.policy_sync_age_seconds,
            principals_sync_age_seconds: m.principals_sync_age_seconds,
            connector_sync_age_seconds: m.connector_sync_age_seconds,
            gateway_pdp_unreachable: m.gateway_pdp_unreachable,
        })
    }

    /// Its heartbeats over `window`, bucketed and oldest first. Buckets with no heartbeat are
    /// left out, so time the install wasn't reporting shows as a gap.
    async fn metric_history(&self, ctx: &Context<'_>, window: MetricWindow) -> Result<Vec<MetricSample>> {
        let samples = store(ctx)?.samples(self.0.id, window.into(), Utc::now()).await?;
        Ok(samples
            .into_iter()
            .map(|s| MetricSample {
                at: UtcTimestamp(s.at),
                decisions_per_second: s.decisions_per_second,
                pdp_p99_ms: s.pdp_p99_ms,
                error_rate: s.error_rate,
                kafka_lag: s.kafka_lag,
                clickhouse_disk_used_percent: s.clickhouse_disk_used_percent,
                containment_feed_age_seconds: s.containment_feed_age_seconds,
                policy_sync_age_seconds: s.policy_sync_age_seconds,
                principals_sync_age_seconds: s.principals_sync_age_seconds,
                connector_sync_age_seconds: s.connector_sync_age_seconds,
                gateway_pdp_unreachable: s.gateway_pdp_unreachable,
                degraded: s.degraded,
            })
            .collect())
    }

    /// Chart versions the install has run, newest first: its upgrade history.
    async fn version_history(&self, ctx: &Context<'_>) -> Result<Vec<VersionSeen>> {
        let versions = store(ctx)?.versions(self.0.id).await?;
        Ok(versions
            .into_iter()
            .map(|(chart_version, at)| VersionSeen { chart_version, first_seen_at: UtcTimestamp(at) })
            .collect())
    }

    /// The staff member who registered it.
    async fn created_by(&self) -> Actor {
        Actor(awzy_model::Actor::awzy_staff(&self.0.created_by))
    }

    async fn created_at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.created_at)
    }

    /// When it was retired; null while it's active.
    async fn retired_at(&self) -> Option<UtcTimestamp> {
        self.0.retired_at.map(UtcTimestamp)
    }

    /// The staff member who retired it.
    async fn retired_by(&self) -> Option<Actor> {
        self.0.retired_by.as_deref().map(|u| Actor(awzy_model::Actor::awzy_staff(u)))
    }
}

/// One awzy service (or ClickHouse) running in an install.
pub struct DeployedService(pub ServiceStatus);

#[Object]
impl DeployedService {
    /// e.g. awzy-pdp.
    async fn name(&self) -> &'static str {
        self.0.service.as_str()
    }

    async fn version(&self) -> Option<&str> {
        self.0.version.as_deref()
    }

    async fn image_digest(&self) -> Option<&str> {
        self.0.image_digest.as_deref()
    }

    async fn replicas_desired(&self) -> u32 {
        self.0.replicas_desired
    }

    async fn replicas_ready(&self) -> u32 {
        self.0.replicas_ready
    }

    #[graphql(name = "restarts24h")]
    async fn restarts_24h(&self) -> Option<u32> {
        self.0.restarts_24h
    }

    /// The service's own readiness check passes and every desired replica is ready.
    async fn healthy(&self) -> bool {
        self.0.ready && self.0.replicas_ready >= self.0.replicas_desired
    }
}

#[derive(SimpleObject)]
pub struct InstallMetrics {
    pub decisions_per_second: Option<f64>,
    pub pdp_p99_ms: Option<f64>,
    /// 0 to 1.
    pub error_rate: Option<f64>,
    pub kafka_lag: Option<u64>,
    /// 0 to 100.
    pub clickhouse_disk_used_percent: Option<f64>,
    pub containment_feed_age_seconds: Option<f64>,
    pub policy_sync_age_seconds: Option<f64>,
    pub principals_sync_age_seconds: Option<f64>,
    pub connector_sync_age_seconds: Option<f64>,
    /// Since the previous heartbeat.
    pub gateway_pdp_unreachable: Option<u64>,
}

#[derive(SimpleObject)]
pub struct VersionSeen {
    pub chart_version: String,
    pub first_seen_at: UtcTimestamp,
}

/// The chart version a release channel names (ADR 0094): every publish moves `FAST`; a
/// promotion, after a day's soak in fast, moves `STABLE`.
#[derive(SimpleObject)]
pub struct ChannelVersion {
    pub channel: ReleaseChannel,
    pub chart_version: String,
    /// Who moved it: `github-actions` for a publish, the person who asked for a promotion.
    pub changed_by: String,
    pub changed_at: UtcTimestamp,
}

/// A chart version and the installs running it.
pub struct Release {
    version: String,
    installs: Vec<InstallRecord>,
}

#[Object]
impl Release {
    async fn version(&self) -> &str {
        &self.version
    }

    async fn install_count(&self) -> usize {
        self.installs.len()
    }

    async fn installs(&self) -> Vec<Install> {
        self.installs.iter().cloned().map(Install).collect()
    }
}

#[derive(Interface)]
#[graphql(field(name = "id", ty = "ID"))]
pub enum Node {
    Install(Install),
}

#[derive(Default)]
pub struct FleetQuery;

#[Object]
impl FleetQuery {
    /// Refetches any object by its ID.
    async fn node(&self, ctx: &Context<'_>, id: ID) -> Result<Option<Node>> {
        let Ok(id) = id.parse::<InstallId>() else { return Ok(None) };
        Ok(store(ctx)?.get(id).await?.map(|r| Node::Install(Install(r))))
    }

    /// Every registered install, in registration order.
    async fn installs(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        first: Option<i32>,
    ) -> Result<Connection<String, Install>> {
        let store = store(ctx)?.clone();
        query(after, None, first, None, |after: Option<String>, _before: Option<String>, first, _last| async move {
            let after = after.map(|a| a.parse::<InstallId>()).transpose()?;
            let limit = first.unwrap_or(DEFAULT_PAGE).min(MAX_PAGE);
            let mut rows = store.list(after, limit as i64 + 1).await?;
            let has_next = rows.len() > limit;
            rows.truncate(limit);
            let mut connection = Connection::new(after.is_some(), has_next);
            connection.edges.extend(rows.into_iter().map(|r| Edge::new(r.id.to_string(), Install(r))));
            Ok::<_, async_graphql::Error>(connection)
        })
        .await
    }

    /// Chart versions installs report, newest first. Installs that haven't reported a version
    /// aren't counted.
    async fn releases(&self, ctx: &Context<'_>) -> Result<Vec<Release>> {
        let mut by_version: BTreeMap<String, Vec<InstallRecord>> = BTreeMap::new();
        for install in store(ctx)?.list(None, 10_000).await? {
            if let Some(v) = install.last_heartbeat.as_ref().and_then(|h| h.chart_version.clone()) {
                by_version.entry(v).or_default().push(install);
            }
        }
        let mut releases: Vec<Release> =
            by_version.into_iter().map(|(version, installs)| Release { version, installs }).collect();
        releases.sort_by(|a, b| fleet::compare_versions(&b.version, &a.version));
        Ok(releases)
    }

    /// Each channel's chart version, stable first. A channel never moved isn't listed.
    async fn channels(&self, ctx: &Context<'_>) -> Result<Vec<ChannelVersion>> {
        Ok(store(ctx)?
            .channel_versions()
            .await?
            .into_iter()
            .map(|c| ChannelVersion {
                channel: c.channel.into(),
                chart_version: c.chart_version,
                changed_by: c.changed_by,
                changed_at: UtcTimestamp(c.changed_at),
            })
            .collect())
    }

    /// The services an install can report, for filters and empty states.
    async fn known_services(&self) -> Vec<&'static str> {
        WireService::ALL.iter().map(|s| s.as_str()).collect()
    }
}

#[derive(InputObject)]
pub struct RegisterInstallInput {
    /// Lowercase slug, e.g. acme.
    pub client: String,
    /// Lowercase slug, e.g. prod.
    pub environment: String,
    /// aws, gcp, or azure.
    pub cloud: String,
    pub region: String,
    pub channel: ReleaseChannel,
    /// Required for PINNED, and only then.
    pub pinned_version: Option<String>,
}

#[derive(SimpleObject)]
pub struct RegisterInstallPayload {
    pub install: Install,
    /// The install's reporting token. Shown once: put it in the install's values now.
    pub reporting_token: String,
    /// Its pull token for the release registry (ADR 0028), shown once like the reporting token.
    /// Null when this portal doesn't issue pull tokens.
    pub pull_token: Option<String>,
}

#[derive(InputObject)]
pub struct RetireInstallInput {
    pub install_qid: ID,
}

#[derive(SimpleObject)]
pub struct RetireInstallPayload {
    pub install: Install,
}

#[derive(Default)]
pub struct FleetMutation;

#[Object]
impl FleetMutation {
    /// Registers an install so its heartbeats are accepted.
    async fn register_install(&self, ctx: &Context<'_>, input: RegisterInstallInput) -> Result<RegisterInstallPayload> {
        let staff = ctx.data::<Staff>().map_err(|_| "sign in first")?;
        let new = NewInstall {
            client: input.client,
            environment: input.environment,
            cloud: input.cloud,
            region: input.region,
            channel: input.channel.into(),
            pinned_version: input.pinned_version,
        };
        match store(ctx)?.register(&new, &staff.username).await {
            Ok((record, token)) => {
                tracing::info!(install = %record.id, client = %record.client, environment = %record.environment, by = %staff.username, "install registered");
                let pull_token = store(ctx)?.issue_pull_token(record.id, &staff.username).await?;
                Ok(RegisterInstallPayload { install: Install(record), reporting_token: token, pull_token })
            }
            Err(e @ (RegisterError::Invalid(_) | RegisterError::Exists { .. })) => Err(e.to_string().into()),
            Err(e) => Err(e.into()),
        }
    }

    /// Retires an install: its reporting and pull tokens stop working at once, it leaves the
    /// fleet views, and its client and environment can be registered again.
    async fn retire_install(&self, ctx: &Context<'_>, input: RetireInstallInput) -> Result<RetireInstallPayload> {
        let staff = ctx.data::<Staff>().map_err(|_| "sign in first")?;
        let id: InstallId = input.install_qid.parse().map_err(|_| "not an install ID")?;
        let record = store(ctx)?.retire(id, &staff.username).await?.ok_or("no active install with that ID")?;
        tracing::warn!(install = %record.id, client = %record.client, environment = %record.environment, by = %staff.username, "install retired");
        Ok(RetireInstallPayload { install: Install(record) })
    }
}
