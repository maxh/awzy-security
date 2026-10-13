//! Contained principals, from awzy-challenge's containment feed.
//!
//! The PDP follows `WatchContainments` and keeps the active list in memory, so a normal check
//! makes no network call for it. The feed sends a heartbeat every half second; if none has
//! arrived for [`STALE_AFTER`], each check asks awzy-challenge directly until the feed recovers.
//! When awzy-challenge can't answer either, the last list stands in for up to `max_stale` after
//! the feed was last heard (ADR 0119), long enough to ride out a restart or a database failover.
//! Past that a check fails as unavailable rather than missing a containment.

use awzy_service_auth::{grpc_channel, AuthChannel};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use awzy_authz_proto::challenge::v1::{
    challenge_service_client::ChallengeServiceClient, Containment, ListContainmentsRequest, WatchContainmentsRequest,
};
use awzy_model::EntityRef;
use tonic::transport::Channel;

/// Missing heartbeats this long makes the in-memory list untrusted.
pub const STALE_AFTER: Duration = Duration::from_secs(2);

/// How long a direct lookup may take while the feed is stale.
const LOOKUP_TIMEOUT: Duration = Duration::from_millis(250);

/// How long the last list stands in after the feed was last heard, when awzy-challenge can't be
/// asked either. Longer than a managed Postgres failover (ADR 0119).
pub const DEFAULT_MAX_STALE: Duration = Duration::from_secs(300);

/// After a direct lookup fails, checks use the last list without asking for this long, so an
/// outage doesn't add the lookup timeout to every check.
const PAUSE_AFTER_FAILED_LOOKUP: Duration = Duration::from_secs(1);

fn key(r: &EntityRef) -> String {
    format!("{}::{}", r.kind, r.id)
}

/// The active list, as awzy-challenge sent it, so a starting peer can be given it unchanged.
#[derive(Default)]
struct Active {
    version: i64,
    list: Vec<Containment>,
    /// `Type::id` to the containment's QID.
    by_principal: HashMap<String, String>,
}

pub struct Containments {
    active: ArcSwap<Active>,
    /// None without awzy-challenge: then nobody is contained and it's always ready.
    client: Option<ChallengeServiceClient<AuthChannel>>,
    loaded: AtomicBool,
    started: Instant,
    /// Milliseconds after `started` when the feed was last heard from; negative for a list a
    /// peer had before this PDP started.
    heard_at_ms: AtomicI64,
    /// Milliseconds after `started` before which a stale feed doesn't ask awzy-challenge.
    lookups_paused_until_ms: AtomicI64,
    max_stale: Duration,
    /// Moves with each list, so a cached query plan is dropped when anyone is contained or
    /// lifted (ADR 0113).
    epoch: AtomicU64,
}

impl Containments {
    pub fn none() -> Self {
        Self::new(None)
    }

    /// Not ready until the first list arrives from awzy-challenge at `endpoint`.
    pub fn from_challenge(endpoint: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?.connect_lazy();
        Ok(Self::new(Some(ChallengeServiceClient::new(grpc_channel(channel, "awzy-challenge")))))
    }

    fn new(client: Option<ChallengeServiceClient<AuthChannel>>) -> Self {
        Self {
            active: ArcSwap::from_pointee(Active::default()),
            loaded: AtomicBool::new(client.is_none()),
            client,
            started: Instant::now(),
            heard_at_ms: AtomicI64::new(0),
            lookups_paused_until_ms: AtomicI64::new(0),
            max_stale: DEFAULT_MAX_STALE,
            epoch: AtomicU64::new(0),
        }
    }

    /// How long the last list stands in when neither the feed nor awzy-challenge answers.
    pub fn with_max_stale(mut self, max_stale: Duration) -> Self {
        self.max_stale = max_stale;
        self
    }

    /// Whether this PDP follows awzy-challenge at all.
    pub fn is_configured(&self) -> bool {
        self.client.is_some()
    }

    pub fn is_ready(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    /// Moves whenever the list is replaced.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Replaces the list: `(principal, containment QID)` pairs.
    pub fn load(&self, active: impl IntoIterator<Item = (EntityRef, String)>) {
        let list = active
            .into_iter()
            .map(|(who, qid)| Containment { qid, principal: Some(who.into()), ..Default::default() })
            .collect();
        self.load_list(0, list, Duration::ZERO);
    }

    /// Replaces the list with one heard `age` ago: from the feed, or from a peer (ADR 0119).
    pub fn load_list(&self, version: i64, list: Vec<Containment>, age: Duration) {
        let by_principal = list
            .iter()
            .filter_map(|c| {
                let who: EntityRef = c.principal.clone()?.into();
                Some((key(&who), c.qid.clone()))
            })
            .collect();
        self.active.store(Arc::new(Active { version, list, by_principal }));
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.heard_ago(age);
        self.loaded.store(true, Ordering::Release);
    }

    /// The list in force, its version, and how long since the feed was heard, for a starting
    /// peer. None before the first list, or without awzy-challenge.
    pub fn held(&self) -> Option<(i64, Vec<Containment>, Duration)> {
        if self.client.is_none() || !self.is_ready() {
            return None;
        }
        let active = self.active.load();
        Some((active.version, active.list.clone(), self.age()))
    }

    fn now_ms(&self) -> i64 {
        i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX)
    }

    fn heard(&self) {
        self.heard_ago(Duration::ZERO);
    }

    fn heard_ago(&self, age: Duration) {
        let age = i64::try_from(age.as_millis()).unwrap_or(i64::MAX);
        self.heard_at_ms.store(self.now_ms().saturating_sub(age), Ordering::Release);
    }

    /// Since the feed was last heard from.
    fn age(&self) -> Duration {
        let ms = self.now_ms().saturating_sub(self.heard_at_ms.load(Ordering::Acquire));
        Duration::from_millis(u64::try_from(ms).unwrap_or(0))
    }

    fn is_fresh(&self) -> bool {
        self.age() <= STALE_AFTER
    }

    fn in_list(&self, who: &[&EntityRef]) -> Vec<String> {
        let active = self.active.load();
        who.iter().filter_map(|w| active.by_principal.get(&key(w)).cloned()).collect()
    }

    /// The containment QIDs of any of `who` that are contained.
    pub async fn find(&self, who: &[&EntityRef]) -> anyhow::Result<Vec<String>> {
        let Some(client) = &self.client else { return Ok(vec![]) };
        if self.is_fresh() {
            return Ok(self.in_list(who));
        }
        if self.now_ms() >= self.lookups_paused_until_ms.load(Ordering::Acquire) {
            match self.look_up(client, who).await {
                Ok(found) => return Ok(found),
                Err(error) => {
                    // At most once per pause, so an outage doesn't flood the log.
                    tracing::warn!(%error, "the containment feed is stale and awzy-challenge didn't answer");
                    let pause = i64::try_from(PAUSE_AFTER_FAILED_LOOKUP.as_millis()).unwrap_or(i64::MAX);
                    self.lookups_paused_until_ms.store(self.now_ms().saturating_add(pause), Ordering::Release);
                }
            }
        }
        let age = self.age();
        anyhow::ensure!(
            age <= self.max_stale,
            "the containment feed hasn't been heard for {}s and awzy-challenge can't be reached",
            age.as_secs()
        );
        crate::metrics::CONTAINMENTS_STALE_SERVED.inc();
        Ok(self.in_list(who))
    }

    /// Asks awzy-challenge about each of `who`, while the feed is stale.
    async fn look_up(
        &self,
        client: &ChallengeServiceClient<AuthChannel>,
        who: &[&EntityRef],
    ) -> anyhow::Result<Vec<String>> {
        let mut found = Vec::new();
        for w in who {
            let res = tokio::time::timeout(
                LOOKUP_TIMEOUT,
                client.clone().list_containments(ListContainmentsRequest {
                    active_only: true,
                    principal: Some((*w).clone().into()),
                    limit: 1,
                    // Recorded, not applied (ADR 0066).
                    include_monitor: false,
                }),
            )
            .await
            .map_err(|_| anyhow::anyhow!("the containment feed is stale and awzy-challenge didn't answer in time"))??;
            found.extend(res.into_inner().containments.into_iter().map(|c| c.qid));
        }
        Ok(found)
    }

    /// Follows the feed forever, reconnecting after errors.
    pub async fn follow(self: Arc<Self>) {
        let Some(client) = self.client.clone() else { return };
        loop {
            if let Err(error) = self.follow_once(client.clone()).await {
                tracing::warn!(%error, "containment feed from awzy-challenge broke; reconnecting");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn follow_once(&self, mut client: ChallengeServiceClient<AuthChannel>) -> anyhow::Result<()> {
        let mut stream = client.watch_containments(WatchContainmentsRequest {}).await?.into_inner();
        while let Some(msg) = stream.message().await? {
            if msg.changed {
                let count = msg.active.len();
                self.load_list(msg.version, msg.active, Duration::ZERO);
                tracing::info!(version = msg.version, active = count, "loaded containments");
            } else {
                self.heard();
            }
        }
        anyhow::bail!("awzy-challenge closed the containment feed")
    }
}

/// The containment feed's freshness (target: applied within 2 s), and how many principals are
/// contained. Without awzy-challenge there's no feed to report.
impl awzy_server::metrics::Source for Containments {
    fn render(&self, out: &mut String) {
        if self.client.is_none() {
            return;
        }
        let age = self.age();
        out.push_str(
            "# HELP awzy_pdp_containment_feed_age_seconds Seconds since the containment feed was last heard from.\n",
        );
        out.push_str("# TYPE awzy_pdp_containment_feed_age_seconds gauge\n");
        out.push_str(&format!("awzy_pdp_containment_feed_age_seconds {:.3}\n", age.as_secs_f64()));
        out.push_str("# HELP awzy_pdp_containments_active Principals contained right now.\n");
        out.push_str("# TYPE awzy_pdp_containments_active gauge\n");
        out.push_str(&format!("awzy_pdp_containments_active {}\n", self.active.load().list.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn without_awzy_challenge_no_one_is_contained() {
        let c = Containments::none();
        assert!(c.is_ready());
        assert!(c.find(&[&EntityRef::new("Human", "alice")]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_fresh_list_answers_from_memory() {
        let c = Containments::from_challenge("http://127.0.0.1:9").unwrap();
        assert!(!c.is_ready(), "not ready before the first list");
        c.load([(EntityRef::new("ServiceAccount", "sales-genie"), "containment_1".to_string())]);
        assert!(c.is_ready());
        let found =
            c.find(&[&EntityRef::new("ServiceAccount", "sales-genie"), &EntityRef::new("Human", "bob")]).await.unwrap();
        assert_eq!(found, ["containment_1"]);
    }

    #[tokio::test]
    async fn a_stale_list_stands_in_while_awzy_challenge_cannot_answer() {
        let c = Containments::from_challenge("http://127.0.0.1:9").unwrap();
        c.load([(EntityRef::new("Human", "mallory"), "containment_1".to_string())]);
        c.heard_ago(STALE_AFTER + Duration::from_secs(1));
        // Nothing listens on port 9, so the direct lookup fails and the last list answers.
        let found = c.find(&[&EntityRef::new("Human", "mallory")]).await.unwrap();
        assert_eq!(found, ["containment_1"]);
    }

    #[tokio::test]
    async fn past_its_limit_a_stale_list_fails_the_check() {
        let c = Containments::from_challenge("http://127.0.0.1:9").unwrap().with_max_stale(Duration::from_secs(10));
        c.load([]);
        c.heard_ago(Duration::from_secs(11));
        assert!(c.find(&[&EntityRef::new("Human", "alice")]).await.is_err(), "never \"not contained\"");
    }

    #[tokio::test]
    async fn a_list_from_a_peer_keeps_its_age() {
        let c = Containments::from_challenge("http://127.0.0.1:9").unwrap();
        assert!(c.held().is_none(), "nothing to pass on before the first list");
        let mallory = EntityRef::new("Human", "mallory");
        let list = vec![Containment {
            qid: "containment_2".into(),
            principal: Some(mallory.clone().into()),
            ..Default::default()
        }];
        c.load_list(4, list, Duration::from_secs(30));
        assert!(c.is_ready());
        let (version, held, age) = c.held().unwrap();
        assert_eq!((version, held.len()), (4, 1));
        assert!(age >= Duration::from_secs(30), "the peer's age carries over: {age:?}");
        assert_eq!(c.find(&[&mallory]).await.unwrap(), ["containment_2"]);
    }
}
