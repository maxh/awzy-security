//! The containment feed: every active containment, pushed to the PDPs as it changes.
//!
//! Each replica keeps the list in memory. It reloads when Postgres notifies a change, from any
//! replica, and every few seconds in case a notification was missed. Each `WatchContainments`
//! stream gets the whole list on connect and on every change, and a heartbeat in between, so a
//! PDP can tell a quiet feed from a broken one.
//!
//! A replica vouches for its list only while it can read Postgres: once it hasn't for
//! [`CONFIRMED_FOR`], its streams go quiet, so the PDPs on them go stale and ask awzy-challenge
//! directly instead of trusting a list another replica may have moved past. The P model beside
//! this file (`feed.p`, ADR 0126) checks that.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use awzy_model::ChallengeId;
use sqlx::postgres::PgListener;
use tokio::sync::{broadcast, watch};

use crate::store::{ActiveContainments, ChallengeStore, StoreError, CONTAINMENT_CHANNEL};

/// Each new or changed challenge's ID, notified by the `challenges` table's trigger.
pub const CHALLENGE_CHANNEL: &str = "awzy_challenges";

/// How often a stream sends a heartbeat when nothing changed. PDPs treat the feed as stale
/// after missing a few.
pub const HEARTBEAT: Duration = Duration::from_millis(500);

/// Reload this often even without a notification.
const RELOAD_EVERY: Duration = Duration::from_secs(5);

/// How long after its last read of Postgres a replica still sends its list as current: two
/// reloads.
pub const CONFIRMED_FOR: Duration = Duration::from_secs(10);

pub struct Feed {
    /// None until the first load: a stream never sends a list it hasn't read.
    tx: watch::Sender<Option<Arc<ActiveContainments>>>,
    /// Challenges as they change, for `WatchChallenges`. A watcher that falls this far behind
    /// misses some, and catches up from its next list.
    challenges: broadcast::Sender<ChallengeId>,
    /// When the last read that succeeded began, newer list or not.
    confirmed: Mutex<Option<Instant>>,
}

impl Default for Feed {
    fn default() -> Self {
        Self { tx: watch::Sender::new(None), challenges: broadcast::Sender::new(1024), confirmed: Mutex::new(None) }
    }
}

impl Feed {
    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<ActiveContainments>>> {
        self.tx.subscribe()
    }

    /// Challenges' IDs as they're opened or change.
    pub fn challenges(&self) -> broadcast::Receiver<ChallengeId> {
        self.challenges.subscribe()
    }

    /// Whether the list has been loaded once.
    pub fn is_loaded(&self) -> bool {
        self.tx.borrow().is_some()
    }

    /// Whether the list may go out as current: loaded, and read from Postgres within
    /// [`CONFIRMED_FOR`].
    pub fn vouches(&self) -> bool {
        let confirmed = *self.confirmed.lock().expect("feed confirmation lock");
        self.is_loaded() && confirmed.is_some_and(|at| at.elapsed() <= CONFIRMED_FOR)
    }

    /// Reloads the list and publishes it if the version moved. Returns true if it did.
    pub async fn refresh(&self, store: &ChallengeStore) -> Result<bool, StoreError> {
        let started = Instant::now();
        let latest = store.active_containments().await?;
        // From when the read began, since Postgres may have moved on while it ran.
        let mut confirmed = self.confirmed.lock().expect("feed confirmation lock");
        *confirmed = Some(confirmed.map_or(started, |at| at.max(started)));
        drop(confirmed);
        Ok(self.tx.send_if_modified(|current| {
            let newer = current.as_ref().is_none_or(|c| latest.version > c.version);
            if newer {
                *current = Some(Arc::new(latest));
            }
            newer
        }))
    }

    /// Follows Postgres notifications forever, reconnecting after errors.
    pub async fn follow(self: Arc<Self>, store: Arc<ChallengeStore>) {
        loop {
            if let Err(error) = self.follow_once(&store).await {
                tracing::warn!(%error, "containment feed lost its Postgres listener; reconnecting");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn follow_once(&self, store: &ChallengeStore) -> anyhow::Result<()> {
        let mut listener = PgListener::connect_with(store.pool()).await?;
        listener.listen_all([CONTAINMENT_CHANNEL, CHALLENGE_CHANNEL]).await?;
        // After listening, so a change made in between is either notified or in this load.
        self.refresh_logged(store).await;
        // An interval, not a sleep in the select: challenges notify often, and each would restart
        // a sleep, so the backstop reload, and the confirmation it renews, would never come.
        let mut reload = tokio::time::interval(RELOAD_EVERY);
        reload.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        reload.tick().await;
        loop {
            tokio::select! {
                notification = listener.recv() => {
                    let notification = notification?;
                    if notification.channel() == CHALLENGE_CHANNEL {
                        if let Ok(id) = notification.payload().parse::<uuid::Uuid>() {
                            // No one watching is fine: the send just finds no receivers.
                            let _ = self.challenges.send(ChallengeId::from_uuid(id));
                        }
                        continue;
                    }
                    self.refresh_logged(store).await;
                }
                _ = reload.tick() => self.refresh_logged(store).await,
            }
        }
    }

    #[cfg(test)]
    fn confirmed_ago(&self, ago: Duration) {
        *self.confirmed.lock().expect("feed confirmation lock") = Instant::now().checked_sub(ago);
    }

    async fn refresh_logged(&self, store: &ChallengeStore) {
        match self.refresh(store).await {
            Ok(true) => {
                if let Some(c) = self.tx.borrow().as_ref() {
                    tracing::info!(version = c.version, active = c.active.len(), "containment feed updated");
                }
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "couldn't reload containments; keeping the current list"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> Feed {
        let feed = Feed::default();
        feed.tx.send_replace(Some(Arc::new(ActiveContainments { version: 3, active: vec![] })));
        feed
    }

    #[test]
    fn a_replica_vouches_for_a_list_it_read_recently() {
        let feed = loaded();
        assert!(!feed.vouches(), "never read from Postgres");
        feed.confirmed_ago(Duration::from_secs(1));
        assert!(feed.vouches());
    }

    #[test]
    fn a_replica_that_cannot_read_postgres_stops_vouching() {
        let feed = loaded();
        feed.confirmed_ago(CONFIRMED_FOR + Duration::from_secs(1));
        assert!(!feed.vouches(), "its list may be behind another replica's");
    }

    #[test]
    fn nothing_is_vouched_for_before_the_first_load() {
        let feed = Feed::default();
        feed.confirmed_ago(Duration::ZERO);
        assert!(!feed.vouches());
    }
}
