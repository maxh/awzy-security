//! When a service's Postgres migrations run (ADR 0083).
//!
//! In an install a release Job applies them before the services roll (`<service> migrate`), so a
//! failed migration stops the sync rather than crash-looping pods, and a service only checks that
//! the schema has what it needs. Run locally, a service still applies its own at start.

use sqlx::{migrate::Migrator, PgPool};

/// The service was started as `<binary> migrate`: apply the migrations and exit.
pub fn requested() -> bool {
    std::env::args().nth(1).as_deref() == Some("migrate")
}

/// At startup: apply the migrations, unless `AWZY_POSTGRES_MIGRATE=check` (the chart's setting),
/// where a release Job has applied them and the service only checks.
pub async fn on_start(migrator: &Migrator, pool: &PgPool) -> anyhow::Result<()> {
    match std::env::var("AWZY_POSTGRES_MIGRATE").as_deref() {
        Ok("check") => ensure_applied(migrator, pool).await,
        Ok("start") | Err(_) => Ok(migrator.run(pool).await?),
        Ok(other) => anyhow::bail!("AWZY_POSTGRES_MIGRATE is start or check, not {other:?}"),
    }
}

/// Fails unless every one of `migrator`'s migrations has been applied. A newer release's extra
/// migrations are fine: during a rollout the older version runs against the newer schema.
pub async fn ensure_applied(migrator: &Migrator, pool: &PgPool) -> anyhow::Result<()> {
    let applied: Vec<i64> = sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success")
        .fetch_all(pool)
        .await
        .map_err(|e| anyhow::anyhow!("can't read the applied migrations (has the migrate Job run?): {e}"))?;
    let missing: Vec<String> = migrator
        .iter()
        .filter(|m| !applied.contains(&m.version))
        .map(|m| format!("{} {}", m.version, m.description))
        .collect();
    anyhow::ensure!(
        missing.is_empty(),
        "the database is behind this release, which needs migrations {}: the migrate Job runs them first",
        missing.join(", ")
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    use super::*;

    /// Against a fresh Postgres schema: the check fails before migrating and passes after.
    #[tokio::test]
    async fn the_check_passes_only_once_the_migrations_are_applied() {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return;
        };
        let unique = format!("{}_{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos());
        let schema = format!("test_migrations_{unique}");
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&PgPool::connect(&url).await.unwrap()).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(1).connect_with(options).await.unwrap();
        let dir = std::env::temp_dir().join(format!("awzy-migrations-{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0001_things.sql"), "CREATE TABLE things (id uuid PRIMARY KEY);").unwrap();
        let migrator = Migrator::new(dir.as_path()).await.unwrap();

        let err = ensure_applied(&migrator, &pool).await.unwrap_err();
        assert!(err.to_string().contains("migrate Job"), "{err}");
        migrator.run(&pool).await.unwrap();
        ensure_applied(&migrator, &pool).await.unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
