//! Which chart version an install may pull (ADR 0094): its release channel's (`stable` or
//! `fast`), or the one it's pinned to. The corp portal keeps both, in the installs and
//! release_channel_changes tables of the public schema this proxy's own schema sits beside.

use awzy_model::InstallId;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub struct ChannelStore {
    pool: PgPool,
}

impl ChannelStore {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Ok(Self { pool: PgPoolOptions::new().max_connections(5).connect(url).await? })
    }

    /// The version `install` may pull, or None while its channel names none (or it's retired).
    pub async fn chart_version(&self, install: InstallId) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT CASE i.channel WHEN 'pinned' THEN i.pinned_version ELSE (
                 SELECT c.chart_version FROM public.release_channel_changes c WHERE c.channel = i.channel
                 ORDER BY c.changed_at DESC, c.id DESC LIMIT 1) END
             FROM public.installs i WHERE i.id = $1 AND i.retired_at IS NULL",
        )
        .bind(install.uuid())
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
    }
}
