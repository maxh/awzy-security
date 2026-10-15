//! Contained principals, for the outage fallback (ADR 0030, ADR 0054).
//!
//! The PDP denies contained principals itself, so this matters only while the PDP is down, when
//! the gateway replays recent allows. The gateway follows awzy-challenge's containment feed,
//! forgets a contained principal's cached allows as soon as the list names them, and never replays
//! one for anyone on its last-known list. So a principal contained before an outage is never
//! replayed.
//!
//! The last-known list keeps counting while the feed is down. A containment issued during an
//! outage can't reach a gateway that's cut off, and the replay TTL bounds that exposure, as ADR
//! 0030 accepts. A gateway that has never received a list replays nothing.
//!
//! The list never goes back to an older version, since the replica a gateway reconnects to may
//! lag the one it left. The P model beside this file (`containments.p`, ADR 0126) checks that.

use std::{
    collections::HashSet,
    sync::{Arc, RwLock},
    time::Duration,
};

use awzy_authz_proto::challenge::v1::{challenge_service_client::ChallengeServiceClient, WatchContainmentsRequest};
use awzy_model::EntityRef;
use awzy_service_auth::AuthChannel;

/// `Type::id`, as the allow cache writes principals.
pub(crate) fn key(r: &EntityRef) -> String {
    format!("{}::{}", r.kind, r.id)
}

pub struct Containments {
    /// None until the first list arrives; then its version and the principals it names.
    known: RwLock<Option<(i64, HashSet<String>)>>,
    /// False without awzy-challenge: then no one is contained and replays are as before.
    configured: bool,
}

/// Whether a recent allow may be replayed for a principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replay {
    Allowed,
    /// The principal, or the person it acts for, is contained.
    Contained,
    /// No containment list has arrived yet, so the gateway can't tell.
    Unknown,
}

impl Containments {
    /// Without awzy-challenge.
    pub fn none() -> Self {
        Self { known: RwLock::new(Some((0, HashSet::new()))), configured: false }
    }

    /// Replays nothing until the first list from awzy-challenge arrives.
    pub fn awaiting_feed() -> Self {
        Self { known: RwLock::new(None), configured: true }
    }

    pub fn is_configured(&self) -> bool {
        self.configured
    }

    /// Whether a list at `version` would replace the one held: none is held, or it's no older.
    pub fn takes(&self, version: i64) -> bool {
        self.known.read().expect("containments lock").as_ref().is_none_or(|(held, _)| version >= *held)
    }

    /// Replaces the list, unless the one held is newer. Returns whether it did.
    pub fn load(&self, version: i64, contained: HashSet<String>) -> bool {
        let mut known = self.known.write().expect("containments lock");
        if known.as_ref().is_some_and(|(held, _)| version < *held) {
            return false;
        }
        *known = Some((version, contained));
        true
    }

    /// Whether a recent allow may be replayed for `who` (the principal, and the person it acts for).
    pub fn replay(&self, who: &[&EntityRef]) -> Replay {
        match self.known.read().expect("containments lock").as_ref() {
            None => Replay::Unknown,
            Some((_, set)) if who.iter().any(|w| set.contains(&key(w))) => Replay::Contained,
            Some(_) => Replay::Allowed,
        }
    }
}

/// Follows the feed forever, reconnecting after errors. `forget` drops cached allows for the
/// principals in each new list.
pub async fn follow(
    containments: Arc<Containments>,
    client: ChallengeServiceClient<AuthChannel>,
    forget: impl Fn(&HashSet<String>) + Send + 'static,
) {
    loop {
        if let Err(error) = follow_once(&containments, client.clone(), &forget).await {
            tracing::warn!(%error, "containment feed from awzy-challenge broke; keeping the last list and reconnecting");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn follow_once(
    containments: &Containments,
    mut client: ChallengeServiceClient<AuthChannel>,
    forget: &impl Fn(&HashSet<String>),
) -> anyhow::Result<()> {
    let mut stream = client.watch_containments(WatchContainmentsRequest {}).await?.into_inner();
    while let Some(msg) = stream.message().await? {
        if msg.changed {
            // Only the feed loads lists, so nothing can change the one held between here and load.
            if !containments.takes(msg.version) {
                tracing::warn!(version = msg.version, "refused an older containment list; this replica is behind");
                continue;
            }
            let contained: HashSet<String> =
                msg.active.iter().filter_map(|c| c.principal.clone()).map(|s| key(&s.into())).collect();
            // Forget first, so there's no moment when the list names someone whose allows remain.
            forget(&contained);
            tracing::info!(version = msg.version, active = contained.len(), "loaded containments");
            containments.load(msg.version, contained);
        }
    }
    anyhow::bail!("awzy-challenge closed the containment feed")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use awzy_authz_proto::challenge::v1::ContainRequest;
    use awzy_challenge::{approvals::Approvals, Access, ChallengeStore, Feed, Lifetimes};
    use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

    use super::*;

    /// A real awzy-challenge on Postgres. Run with `--config=local-stack`; skips otherwise.
    async fn awzy_challenge() -> Option<ChallengeServiceClient<AuthChannel>> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&PgPool::connect(&url).await.unwrap()).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(3).connect_with(options).await.unwrap();
        let store = Arc::new(ChallengeStore::new(pool, Lifetimes::default()));
        store.migrate().await.unwrap();
        let feed = Arc::new(Feed::default());
        tokio::spawn(feed.clone().follow(store.clone()));
        let app = awzy_challenge::router(store, feed, Arc::new(Approvals::Off), None, &Access::off());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let channel = tonic::transport::Channel::from_shared(format!("http://{addr}")).unwrap().connect_lazy();
        Some(ChallengeServiceClient::new(awzy_service_auth::grpc_channel(channel, "awzy-challenge")))
    }

    #[tokio::test]
    async fn a_containment_reaches_the_gateway_and_clears_its_allows() {
        let Some(mut client) = awzy_challenge().await else { return };
        let containments = Arc::new(Containments::awaiting_feed());
        let forgotten = Arc::new(std::sync::Mutex::new(Vec::<HashSet<String>>::new()));
        let log = forgotten.clone();
        tokio::spawn(follow(containments.clone(), client.clone(), move |set| log.lock().unwrap().push(set.clone())));

        let agent = EntityRef::new("ServiceAccount", "sales-genie");
        for _ in 0..100 {
            if containments.replay(&[&agent]) != Replay::Unknown {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(containments.replay(&[&agent]), Replay::Allowed, "the first list arrived, and it's empty");

        client
            .contain(ContainRequest {
                principal: Some(agent.clone().into()),
                reason: "bulk export".into(),
                created_by: "alice".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        for _ in 0..100 {
            if containments.replay(&[&agent]) == Replay::Contained {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(containments.replay(&[&agent]), Replay::Contained);
        let last = forgotten.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last, HashSet::from(["ServiceAccount::sales-genie".to_string()]), "its allows were forgotten");
    }

    #[test]
    fn replays_wait_for_a_list_then_skip_the_contained() {
        let c = Containments::awaiting_feed();
        let agent = EntityRef::new("ServiceAccount", "sales-genie");
        let bob = EntityRef::new("Human", "bob");
        assert_eq!(c.replay(&[&agent]), Replay::Unknown, "nothing replays before the first list");

        c.load(1, HashSet::from([key(&bob)]));
        assert_eq!(c.replay(&[&agent]), Replay::Allowed);
        assert_eq!(c.replay(&[&agent, &bob]), Replay::Contained, "an agent acting for a contained person");
        assert_eq!(Containments::none().replay(&[&bob]), Replay::Allowed, "without awzy-challenge, as before");
    }

    #[test]
    fn a_lagging_replica_never_rolls_the_list_back() {
        let c = Containments::awaiting_feed();
        let bob = EntityRef::new("Human", "bob");
        assert!(c.load(5, HashSet::from([key(&bob)])));
        assert!(!c.takes(4) && !c.load(4, HashSet::new()), "an older list is refused");
        assert_eq!(c.replay(&[&bob]), Replay::Contained, "so bob's containment stands");
        assert!(c.load(6, HashSet::new()), "a newer one lifts it");
        assert_eq!(c.replay(&[&bob]), Replay::Allowed);
    }
}
