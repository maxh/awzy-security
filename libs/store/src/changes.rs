//! Waiting for a change instead of polling for one (ADR 0077): a service follows a Postgres
//! NOTIFY channel that its tables' triggers fire, and a long-poll handler waits on it.
//!
//! The counter moves on every notification, and on every reconnect of the listener, since one
//! could have been missed while it was down. A waiter takes the counter before reading what it
//! answers from, so a change committed in between still wakes it.

use std::{sync::Arc, time::Duration};

use sqlx::{postgres::PgListener, PgPool};
use tokio::sync::watch;

pub struct Changes {
    tx: watch::Sender<u64>,
}

impl Default for Changes {
    fn default() -> Self {
        Self { tx: watch::Sender::new(0) }
    }
}

impl Changes {
    /// Where the counter is now. Take it before reading, then [`Self::wait_after`] it.
    pub fn current(&self) -> u64 {
        *self.tx.borrow()
    }

    /// Returns when the counter has moved past `seen`, or after `max`, whichever is first.
    pub async fn wait_after(&self, seen: u64, max: Duration) {
        let mut rx = self.tx.subscribe();
        let _ = tokio::time::timeout(max, rx.wait_for(|n| *n > seen)).await;
    }

    /// Moves the counter, waking every waiter.
    pub fn changed(&self) {
        self.tx.send_modify(|n| *n += 1);
    }

    /// Follows `channel` on `pool` forever, reconnecting after errors.
    pub async fn follow(self: Arc<Self>, pool: PgPool, channel: &'static str) {
        loop {
            if let Err(error) = self.follow_once(&pool, channel).await {
                tracing::warn!(%error, channel, "lost the Postgres listener; reconnecting");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn follow_once(&self, pool: &PgPool, channel: &str) -> anyhow::Result<()> {
        let mut listener = PgListener::connect_with(pool).await?;
        listener.listen(channel).await?;
        // Anything could have changed while no listener was connected.
        self.changed();
        loop {
            listener.recv().await?;
            self.changed();
        }
    }
}

/// The longest a long-poll may wait: below common proxy and load balancer idle timeouts.
pub const MAX_WAIT: Duration = Duration::from_secs(25);

/// A caller's `wait` in seconds, capped at [`MAX_WAIT`].
pub fn wait_for(seconds: Option<u64>) -> Duration {
    Duration::from_secs(seconds.unwrap_or(0)).min(MAX_WAIT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_waiter_wakes_on_a_change_after_what_it_saw() {
        let changes = Arc::new(Changes::default());
        let seen = changes.current();
        let waiter = {
            let changes = changes.clone();
            tokio::spawn(async move { changes.wait_after(seen, Duration::from_secs(10)).await })
        };
        changes.changed();
        tokio::time::timeout(Duration::from_secs(1), waiter).await.expect("woken by the change").unwrap();

        // Already past: returns at once.
        tokio::time::timeout(Duration::from_millis(100), changes.wait_after(seen, Duration::from_secs(10)))
            .await
            .expect("no wait for a change already seen");
    }

    #[tokio::test]
    async fn without_a_change_it_waits_its_maximum() {
        let changes = Changes::default();
        let started = std::time::Instant::now();
        changes.wait_after(changes.current(), Duration::from_millis(50)).await;
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(wait_for(Some(600)), MAX_WAIT);
    }
}
