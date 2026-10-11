//! The decision log, from ClickHouse.
//!
//! [`search`], [`find`], and [`explain`] are the operations; the GraphQL types below and the
//! MCP tools (crate::mcp) both call them, so neither can do more than the other.
//!
//! Every query is bounded: a time window (default the last 24 hours, at most 30 days) and a
//! page size (default 50, at most 200), so a portal request can't scan the whole table.

use async_graphql::{
    connection::{query, Connection, Edge},
    Context, Enum, InputObject, Object, Result, SimpleObject, ID,
};
use awzy_authz_proto::policy::v1::{
    get_bundle_request::Key, policy_service_client::PolicyServiceClient, GetBundleRequest,
};
use awzy_model::DecisionId;
use awzy_portal::UtcTimestamp;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    access::{require, PortalAction, PortalArea},
    clickhouse, Services,
};

const DEFAULT_PAGE: usize = 50;
const MAX_PAGE: usize = 200;
const DEFAULT_WINDOW: Duration = Duration::hours(24);
const MAX_WINDOW: Duration = Duration::days(30);

const COLUMNS: &str = "id, at, principal_type, principal_id, on_behalf_of_type, on_behalf_of_id, human_present, \
    action, resource_type, resource_id, verdict, mode, challenge, policies, on_behalf_of_verdict, on_behalf_of_policies, \
    policy_version, context";

#[derive(::clickhouse::Row, serde::Deserialize, Clone, Debug)]
pub struct DecisionRow {
    #[serde(with = "::clickhouse::serde::uuid")]
    pub id: Uuid,
    #[serde(with = "::clickhouse::serde::chrono::datetime64::millis")]
    pub at: DateTime<Utc>,
    pub principal_type: String,
    pub principal_id: String,
    pub on_behalf_of_type: String,
    pub on_behalf_of_id: String,
    pub human_present: bool,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub verdict: String,
    /// monitor | enforce
    pub mode: String,
    pub challenge: String,
    pub policies: Vec<String>,
    pub on_behalf_of_verdict: String,
    pub on_behalf_of_policies: Vec<String>,
    pub policy_version: i64,
    pub context: String,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, serde::Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(crate = "rmcp::schemars")]
pub enum Verdict {
    Allow,
    Deny,
    Challenge,
}

impl Verdict {
    pub fn parse(s: &str) -> Self {
        match s {
            "allow" => Self::Allow,
            "challenge" => Self::Challenge,
            _ => Self::Deny,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::Challenge => "challenge",
        }
    }
}

#[derive(SimpleObject)]
pub struct EntityRef {
    #[graphql(name = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(SimpleObject)]
pub struct PolicySource {
    /// The policy's @id annotation, or Cedar's generated ID.
    pub id: String,
    pub source: String,
}

/// One authorization decision, as the PDP logged it.
pub struct Decision(pub DecisionRow);

#[Object]
impl Decision {
    /// QID: decision_<uuid>, the ID the PDP returned to the enforcement point.
    pub async fn id(&self) -> ID {
        ID(DecisionId::from_uuid(self.0.id).to_string())
    }

    async fn at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.at)
    }

    async fn principal(&self) -> EntityRef {
        EntityRef { kind: self.0.principal_type.clone(), id: self.0.principal_id.clone() }
    }

    /// Set when a service account acted for a person: that person and their own check.
    async fn on_behalf_of(&self) -> Option<OnBehalfOf> {
        (!self.0.on_behalf_of_id.is_empty()).then(|| OnBehalfOf(self.0.clone()))
    }

    /// Whether a person was there to answer a challenge.
    async fn human_present(&self) -> bool {
        self.0.human_present
    }

    async fn action(&self) -> &str {
        &self.0.action
    }

    async fn resource(&self) -> EntityRef {
        EntityRef { kind: self.0.resource_type.clone(), id: self.0.resource_id.clone() }
    }

    /// What policy decided. For a delegated request, the stricter of the principal's and the
    /// person's checks. In MONITOR mode it was not applied: a DENY is a "would deny".
    async fn verdict(&self) -> Verdict {
        Verdict::parse(&self.0.verdict)
    }

    async fn mode(&self) -> crate::enforcement::Mode {
        crate::enforcement::Mode::parse(&self.0.mode)
    }

    /// When the verdict is CHALLENGE: every challenge kind required.
    async fn challenges(&self) -> Vec<&str> {
        split_challenges(&self.0.challenge)
    }

    /// IDs of the policies that determined the principal's own check.
    async fn policies(&self) -> &[String] {
        &self.0.policies
    }

    /// The policy bundle version in force; 0 means the built-in defaults.
    async fn policy_version(&self) -> i64 {
        self.0.policy_version
    }

    /// The request context, as JSON text.
    async fn context(&self) -> &str {
        &self.0.context
    }

    /// Explain the principal's check: the source of each determining policy, from the bundle the
    /// decision was made under.
    async fn determining_policies(&self, ctx: &Context<'_>) -> Result<Vec<PolicySource>> {
        let policy = ctx.data::<Services>().ok().and_then(|s| s.policy.clone());
        Ok(explain(policy, self.0.policy_version, &self.0.policies).await?)
    }
}

/// The second check a delegated request gets: could the person do this themselves?
pub struct OnBehalfOf(pub DecisionRow);

#[Object]
impl OnBehalfOf {
    async fn human(&self) -> EntityRef {
        EntityRef { kind: self.0.on_behalf_of_type.clone(), id: self.0.on_behalf_of_id.clone() }
    }

    async fn verdict(&self) -> Verdict {
        Verdict::parse(&self.0.on_behalf_of_verdict)
    }

    async fn policies(&self) -> &[String] {
        &self.0.on_behalf_of_policies
    }

    async fn determining_policies(&self, ctx: &Context<'_>) -> Result<Vec<PolicySource>> {
        let policy = ctx.data::<Services>().ok().and_then(|s| s.policy.clone());
        Ok(explain(policy, self.0.policy_version, &self.0.on_behalf_of_policies).await?)
    }
}

#[derive(InputObject, Default)]
pub struct DecisionFilter {
    pub principal_id: Option<String>,
    pub resource_id: Option<String>,
    pub verdict: Option<Verdict>,
    pub mode: Option<crate::enforcement::Mode>,
    /// Defaults to 24 hours ago; at most 30 days ago.
    pub since: Option<UtcTimestamp>,
}

pub async fn get_by_id(ctx: &Context<'_>, id: DecisionId) -> Result<Option<Decision>> {
    require(ctx, PortalAction::View, PortalArea::Decisions).await?;
    Ok(find(&clickhouse(ctx)?, id).await?.map(Decision))
}

#[derive(Default)]
pub struct DecisionQuery;

#[Object]
impl DecisionQuery {
    /// Decisions newest first. Cursors are opaque.
    async fn decisions(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        first: Option<i32>,
        filter: Option<DecisionFilter>,
    ) -> Result<Connection<String, Decision>> {
        require(ctx, PortalAction::View, PortalArea::Decisions).await?;
        let client = clickhouse(ctx)?;
        let filter = filter.unwrap_or_default();
        query(after, None, first, None, |after: Option<String>, _before: Option<String>, first, _last| async move {
            let after = after.as_deref().map(parse_cursor).transpose()?;
            let has_previous = after.is_some();
            let page = search(
                &client,
                SearchParams {
                    principal_id: filter.principal_id,
                    resource_id: filter.resource_id,
                    verdict: filter.verdict,
                    mode: filter.mode.map(Into::into),
                    since: filter.since.map(|t| t.0),
                    after,
                    limit: first.unwrap_or(DEFAULT_PAGE),
                },
            )
            .await?;
            let mut connection = Connection::new(has_previous, page.has_next);
            connection.edges.extend(page.rows.into_iter().map(|r| Edge::new(cursor_for(&r), Decision(r))));
            Ok::<_, async_graphql::Error>(connection)
        })
        .await
    }
}

/// What to search for. `since` and `limit` are clamped to the bounds at the top of this file.
pub struct SearchParams {
    pub principal_id: Option<String>,
    pub resource_id: Option<String>,
    pub verdict: Option<Verdict>,
    pub mode: Option<awzy_model::EnforcementMode>,
    pub since: Option<DateTime<Utc>>,
    /// Continue after this position in `ORDER BY at DESC, id DESC`.
    pub after: Option<(DateTime<Utc>, Uuid)>,
    pub limit: usize,
}

pub struct SearchPage {
    pub rows: Vec<DecisionRow>,
    pub has_next: bool,
}

/// Decisions newest first, within the time window and page size bounds.
pub async fn search(ch: &::clickhouse::Client, p: SearchParams) -> anyhow::Result<SearchPage> {
    let limit = p.limit.clamp(1, MAX_PAGE);
    let since = window_start(p.since, Utc::now());

    let mut sql = format!("SELECT {COLUMNS} FROM decisions WHERE at >= fromUnixTimestamp64Milli(?, 'UTC')");
    let mut strings = Vec::new();
    if let Some(v) = p.principal_id {
        sql.push_str(" AND principal_id = ?");
        strings.push(v);
    }
    if let Some(v) = p.resource_id {
        sql.push_str(" AND resource_id = ?");
        strings.push(v);
    }
    if let Some(v) = p.verdict {
        sql.push_str(" AND verdict = ?");
        strings.push(v.as_str().to_string());
    }
    if let Some(m) = p.mode {
        sql.push_str(" AND mode = ?");
        strings.push(m.as_str().to_string());
    }
    if p.after.is_some() {
        sql.push_str(" AND (at, id) < (fromUnixTimestamp64Milli(?, 'UTC'), toUUID(?))");
    }
    sql.push_str(" ORDER BY at DESC, id DESC LIMIT ?");

    let mut q = ch.query(&sql).bind(since.timestamp_millis());
    for s in &strings {
        q = q.bind(s.as_str());
    }
    if let Some((at, id)) = &p.after {
        q = q.bind(at.timestamp_millis()).bind(id.to_string());
    }
    let mut rows: Vec<DecisionRow> = q.bind((limit + 1) as u64).fetch_all().await?;
    let has_next = rows.len() > limit;
    rows.truncate(limit);
    Ok(SearchPage { rows, has_next })
}

pub async fn find(ch: &::clickhouse::Client, id: DecisionId) -> anyhow::Result<Option<DecisionRow>> {
    let rows: Vec<DecisionRow> = ch
        .query(&format!("SELECT {COLUMNS} FROM decisions WHERE id = toUUID(?) LIMIT 1"))
        .bind(id.uuid().to_string())
        .fetch_all()
        .await?;
    Ok(rows.into_iter().next())
}

/// The source of the policies `ids` in bundle `version` (0: the built-in defaults).
pub async fn explain(
    policy: Option<PolicyServiceClient<awzy_service_auth::AuthChannel>>,
    version: i64,
    ids: &[String],
) -> anyhow::Result<Vec<PolicySource>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let policies = if version == 0 {
        awzy_pdp::DEFAULT_POLICIES.to_string()
    } else {
        let mut policy =
            policy.ok_or_else(|| anyhow::anyhow!("the policy service is not configured (AWZY_POLICY_ADDR)"))?;
        policy
            .get_bundle(GetBundleRequest { key: Some(Key::Version(version)) })
            .await?
            .into_inner()
            .bundle
            .ok_or_else(|| anyhow::anyhow!("awzy-policy returned no bundle"))?
            .policies
    };
    let found = awzy_pdp::policy_sources(&policies, ids)?;
    Ok(found.into_iter().map(|(id, source)| PolicySource { id, source }).collect())
}

/// Decisions since `since` (clamped like search), counted by verdict and mode.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ModeCounts {
    pub total: u64,
    pub monitored: u64,
    pub would_deny: u64,
    pub would_challenge: u64,
    pub enforced_deny: u64,
    pub enforced_challenge: u64,
}

pub async fn mode_counts(
    ch: &::clickhouse::Client,
    since: Option<DateTime<Utc>>,
) -> anyhow::Result<(DateTime<Utc>, ModeCounts)> {
    #[derive(::clickhouse::Row, serde::Deserialize)]
    struct Row {
        total: u64,
        monitored: u64,
        would_deny: u64,
        would_challenge: u64,
        enforced_deny: u64,
        enforced_challenge: u64,
    }
    let since = window_start(since, Utc::now());
    let r: Row = ch
        .query(
            "SELECT count() AS total,
                    countIf(mode = 'monitor') AS monitored,
                    countIf(mode = 'monitor' AND verdict = 'deny') AS would_deny,
                    countIf(mode = 'monitor' AND verdict = 'challenge') AS would_challenge,
                    countIf(mode = 'enforce' AND verdict = 'deny') AS enforced_deny,
                    countIf(mode = 'enforce' AND verdict = 'challenge') AS enforced_challenge
             FROM decisions WHERE at >= fromUnixTimestamp64Milli(?, 'UTC')",
        )
        .bind(since.timestamp_millis())
        .fetch_one()
        .await?;
    Ok((
        since,
        ModeCounts {
            total: r.total,
            monitored: r.monitored,
            would_deny: r.would_deny,
            would_challenge: r.would_challenge,
            enforced_deny: r.enforced_deny,
            enforced_challenge: r.enforced_challenge,
        },
    ))
}

/// The decision log stores challenge kinds comma-separated.
pub fn split_challenges(s: &str) -> Vec<&str> {
    s.split(',').filter(|k| !k.is_empty()).collect()
}

fn window_start(since: Option<DateTime<Utc>>, now: DateTime<Utc>) -> DateTime<Utc> {
    since.unwrap_or(now - DEFAULT_WINDOW).max(now - MAX_WINDOW)
}

/// Opaque to clients: `<at millis>.<uuid>`, the position in `ORDER BY at DESC, id DESC`.
fn cursor_for(r: &DecisionRow) -> String {
    format!("{}.{}", r.at.timestamp_millis(), r.id)
}

fn parse_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid)> {
    let invalid = || async_graphql::Error::new("invalid cursor");
    let (at, id) = cursor.split_once('.').ok_or_else(invalid)?;
    let at = at.parse().ok().and_then(DateTime::from_timestamp_millis).ok_or_else(invalid)?;
    Ok((at, Uuid::parse_str(id).map_err(|_| invalid())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        policies::tests::{serve_fake_policy, FakePolicy},
        Services,
    };

    #[test]
    fn window_is_clamped_to_30_days() {
        let now = Utc::now();
        assert_eq!(window_start(None, now), now - DEFAULT_WINDOW);
        assert_eq!(window_start(Some(now - Duration::days(400)), now), now - MAX_WINDOW);
        assert_eq!(window_start(Some(now - Duration::hours(1)), now), now - Duration::hours(1));
    }

    #[test]
    fn cursors_round_trip() {
        let row = DecisionRow {
            id: Uuid::now_v7(),
            at: DateTime::from_timestamp_millis(1_700_000_000_123).unwrap(),
            principal_type: String::new(),
            principal_id: String::new(),
            on_behalf_of_type: String::new(),
            on_behalf_of_id: String::new(),
            human_present: true,
            action: String::new(),
            resource_type: String::new(),
            resource_id: String::new(),
            verdict: String::new(),
            mode: "monitor".into(),
            challenge: String::new(),
            policies: vec![],
            on_behalf_of_verdict: String::new(),
            on_behalf_of_policies: vec![],
            policy_version: 0,
            context: String::new(),
        };
        assert_eq!(parse_cursor(&cursor_for(&row)).unwrap(), (row.at, row.id));
        assert!(parse_cursor("nonsense").is_err());
    }

    /// With --config=local-stack: filters, pagination, node refetch, and explain over real ClickHouse.
    #[tokio::test]
    async fn search_page_and_explain() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let database = format!("test_{}", Uuid::now_v7().simple());
        let admin = ::clickhouse::Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        admin.query(&format!("CREATE DATABASE {database}")).execute().await.unwrap();
        let ch = admin.clone().with_database(&database);
        // The Kafka queue table is created but never consumes; rows are inserted directly.
        let settings = awzy_clickhouse::Settings {
            kafka_brokers: "localhost:1".into(),
            decisions_topic: format!("unused.{database}"),
            events_topic: format!("unused-events.{database}"),
            consumer_group: database.clone(),
        };
        awzy_clickhouse::migrate(&ch, &settings).await.unwrap();

        let now_ms = Utc::now().timestamp_millis();
        let mut values = Vec::new();
        for i in 0..5 {
            let (principal, verdict, policy) =
                if i < 3 { ("alice", "allow", "security-team-read") } else { ("bob", "deny", "") };
            let id = Uuid::now_v7();
            values.push(format!(
                "('{id}', fromUnixTimestamp64Milli({}, 'UTC'), 'Human', '{principal}', 'read', 'Resource', 'doc{i}', '{verdict}', [{}], true)",
                now_ms - i * 1000,
                if policy.is_empty() { String::new() } else { format!("'{policy}'") }
            ));
        }
        ch.query(&format!(
            "INSERT INTO decisions (id, at, principal_type, principal_id, action, resource_type, resource_id, verdict, policies, human_present) VALUES {}",
            values.join(",")
        ))
        .execute()
        .await
        .unwrap();

        let schema = crate::schema_with(Services {
            policy: Some(serve_fake_policy(FakePolicy::default()).await),
            challenge: None,
            clickhouse: Some(ch),
            pdp: Some(crate::testing::pdp().await),
            context: None,
            bot: None,
        });
        let alice = crate::testing::human("alice", "security");
        let run = |q: &str| {
            let (schema, alice, q) = (schema.clone(), alice.clone(), q.to_string());
            async move { crate::testing::run(&schema, &alice, q).await }
        };

        let first = run(r#"{ decisions(first: 2, filter: { principalId: "alice" }) {
            edges { cursor node { id verdict resource { id } determiningPolicies { id source } } }
            pageInfo { hasNextPage }
        } }"#)
        .await;
        let edges = first["decisions"]["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0]["node"]["resource"]["id"], "doc0", "newest first");
        assert_eq!(first["decisions"]["pageInfo"]["hasNextPage"], true);
        let explained = &edges[0]["node"]["determiningPolicies"][0];
        assert_eq!(explained["id"], "security-team-read");
        assert!(explained["source"].as_str().unwrap().contains("Group::\"security\""));

        let cursor = edges[1]["cursor"].as_str().unwrap();
        let second = run(&format!(
            r#"{{ decisions(first: 2, after: "{cursor}", filter: {{ principalId: "alice" }}) {{ edges {{ node {{ resource {{ id }} }} }} pageInfo {{ hasNextPage }} }} }}"#
        ))
        .await;
        assert_eq!(second["decisions"]["edges"].as_array().unwrap().len(), 1);
        assert_eq!(second["decisions"]["edges"][0]["node"]["resource"]["id"], "doc2");
        assert_eq!(second["decisions"]["pageInfo"]["hasNextPage"], false);

        let denies =
            run(r#"{ decisions(filter: { verdict: DENY }) { edges { node { principal { id } mode } } } }"#).await;
        assert_eq!(denies["decisions"]["edges"].as_array().unwrap().len(), 2);
        assert_eq!(denies["decisions"]["edges"][0]["node"]["mode"], "MONITOR", "rows without a mode count as monitor");

        let summary = run(r#"{ enforcementSummary { total monitored wouldDeny wouldChallenge enforcedDeny } }"#).await;
        assert_eq!(
            summary["enforcementSummary"],
            serde_json::json!({ "total": 5, "monitored": 5, "wouldDeny": 2, "wouldChallenge": 0, "enforcedDeny": 0 })
        );
        let enforced = run(r#"{ decisions(filter: { mode: ENFORCE }) { edges { node { id } } } }"#).await;
        assert!(enforced["decisions"]["edges"].as_array().unwrap().is_empty());

        let id = edges[0]["node"]["id"].as_str().unwrap();
        assert!(id.starts_with("decision_"), "{id}");
        let node = run(&format!(r#"{{ node(id: "{id}") {{ ... on Decision {{ principal {{ id }} }} }} }}"#)).await;
        assert_eq!(node["node"]["principal"]["id"], "alice");

        admin.query(&format!("DROP DATABASE {database}")).execute().await.unwrap();
    }
}
