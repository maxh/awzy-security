//! The containment feed: every active containment, pushed to the PDPs as it changes.
//!
//! Each replica keeps the list in memory. It reloads when Postgres notifies a change, from any
//! replica, and every few seconds in case a notification was missed. Each `WatchContainments`
//! stream gets the whole list on connect and on every change, and a heartbeat in between, so a
//! PDP can tell a quiet feed from a broken one.

use std::{sync::Arc, time::Duration};

use sqlx::postgres::PgListener;
use tokio::sync::watch;

use crate::store::{ActiveContainments, ChallengeStore, StoreError, CONTAINMENT_CHANNEL};

/// How often a stream sends a heartbeat when nothing changed. PDPs treat the feed as stale
/// after missing a few.
pub const HEARTBEAT: Duration = Duration::from_millis(500);

/// Reload this often even without a notification.
const RELOAD_EVERY: Duration = Duration::from_secs(5);

pub struct Feed {
    /// None until the first load: a stream never sends a list it hasn't read.
    tx: watch::Sender<Option<Arc<ActiveContainments>>>,
}

impl Default for Feed {
    fn default() -> Self {
        Self { tx: watch::Sender::new(None) }
    }
}

impl Feed {
    pub fn subscribe(&self) -> watch::Receiver<Option<Arc<ActiveContainments>>> {
        self.tx.subscribe()
    }

    /// Whether the list has been loaded once.
    pub fn is_loaded(&self) -> bool {
        self.tx.borrow().is_some()
    }

    /// Reloads the list and publishes it if the version moved. Returns true if it did.
    pub async fn refresh(&self, store: &ChallengeStore) -> Result<bool, StoreError> {
        let latest = store.active_containments().await?;
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
        listener.listen(CONTAINMENT_CHANNEL).await?;
        // After listening, so a change made in between is either notified or in this load.
        self.refresh_logged(store).await;
        loop {
            tokio::select! {
                notification = listener.recv() => {
                    notification?;
                    self.refresh_logged(store).await;
                }
                _ = tokio::time::sleep(RELOAD_EVERY) => self.refresh_logged(store).await,
            }
        }
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
