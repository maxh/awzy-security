//! Contained accessors, from awzy-challenge's containment feed.
//!
//! The PDP follows `WatchContainments` and keeps the active list in memory, so a normal check
//! makes no network call for it. The feed sends a heartbeat every half second; if none has
//! arrived for [`STALE_AFTER`], each check asks awzy-challenge directly until the feed recovers,
//! and a check it can't answer fails as unavailable rather than missing a containment.

use awzy_service_auth::{grpc_channel, AuthChannel};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use awzy_authz_proto::challenge::v1::{
    challenge_service_client::ChallengeServiceClient, ListContainmentsRequest, WatchContainmentsRequest,
};
use awzy_model::EntityRef;
use tonic::transport::Channel;

/// Missing heartbeats this long makes the in-memory list untrusted.
pub const STALE_AFTER: Duration = Duration::from_secs(2);

/// How long a direct lookup may take while the feed is stale.
const LOOKUP_TIMEOUT: Duration = Duration::from_millis(250);

fn key(r: &EntityRef) -> String {
    format!("{}::{}", r.kind, r.id)
}

pub struct Containments {
    /// `Type::id` to the containment's QID.
    active: ArcSwap<HashMap<String, String>>,
    /// None without awzy-challenge: then nobody is contained and it's always ready.
    client: Option<ChallengeServiceClient<AuthChannel>>,
    loaded: AtomicBool,
    started: Instant,
    /// Milliseconds after `started` when the feed was last heard from.
    heard_at_ms: AtomicU64,
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
            active: ArcSwap::from_pointee(HashMap::new()),
            loaded: AtomicBool::new(client.is_none()),
            client,
            started: Instant::now(),
            heard_at_ms: AtomicU64::new(0),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }

    /// Replaces the list: `(accessor, containment QID)` pairs.
    pub fn load(&self, active: impl IntoIterator<Item = (EntityRef, String)>) {
        self.active.store(Arc::new(active.into_iter().map(|(who, id)| (key(&who), id)).collect()));
        self.heard();
        self.loaded.store(true, Ordering::Release);
    }

    fn heard(&self) {
        self.heard_at_ms.store(self.started.elapsed().as_millis() as u64, Ordering::Release);
    }

    fn is_fresh(&self) -> bool {
        let heard = Duration::from_millis(self.heard_at_ms.load(Ordering::Acquire));
        self.started.elapsed().saturating_sub(heard) <= STALE_AFTER
    }

    /// The containment QIDs of any of `who` that are contained.
    pub async fn find(&self, who: &[&EntityRef]) -> anyhow::Result<Vec<String>> {
        let Some(client) = &self.client else { return Ok(vec![]) };
        if self.is_fresh() {
            let active = self.active.load();
            return Ok(who.iter().filter_map(|w| active.get(&key(w)).cloned()).collect());
        }
        let mut found = Vec::new();
        for w in who {
            let res = tokio::time::timeout(
                LOOKUP_TIMEOUT,
                client.clone().list_containments(ListContainmentsRequest {
                    active_only: true,
                    subject: Some((*w).clone().into()),
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
                self.load(msg.active.into_iter().filter_map(|c| Some((c.subject?.into(), c.qid))));
                tracing::info!(version = msg.version, active = count, "loaded containments");
            } else {
                self.heard();
            }
        }
        anyhow::bail!("awzy-challenge closed the containment feed")
    }
}

/// The containment feed's freshness (target: applied within 2 s), and how many accessors are
/// contained. Without awzy-challenge there's no feed to report.
impl awzy_server::metrics::Source for Containments {
    fn render(&self, out: &mut String) {
        if self.client.is_none() {
            return;
        }
        let heard = Duration::from_millis(self.heard_at_ms.load(Ordering::Acquire));
        let age = self.started.elapsed().saturating_sub(heard);
        out.push_str(
            "# HELP awzy_pdp_containment_feed_age_seconds Seconds since the containment feed was last heard from.\n",
        );
        out.push_str("# TYPE awzy_pdp_containment_feed_age_seconds gauge\n");
        out.push_str(&format!("awzy_pdp_containment_feed_age_seconds {:.3}\n", age.as_secs_f64()));
        out.push_str("# HELP awzy_pdp_containments_active Accessors contained right now.\n");
        out.push_str("# TYPE awzy_pdp_containments_active gauge\n");
        out.push_str(&format!("awzy_pdp_containments_active {}\n", self.active.load().len()));
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
    async fn a_stale_list_is_not_trusted() {
        let c = Containments::from_challenge("http://127.0.0.1:9").unwrap();
        c.load([]);
        c.heard_at_ms.store(0, Ordering::Release);
        tokio::time::sleep(STALE_AFTER + Duration::from_millis(50)).await;
        // Nothing listens on port 9, so the direct lookup fails instead of answering "no one".
        assert!(c.find(&[&EntityRef::new("Human", "alice")]).await.is_err());
    }
}
