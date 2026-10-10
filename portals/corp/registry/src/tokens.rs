//! Pull tokens, one per install, in the `registry` schema of the corp portal's database.

use std::{
    collections::HashMap,
    str::FromStr,
    sync::Mutex,
    time::{Duration, Instant},
};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use awzy_model::InstallId;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    Executor, PgPool,
};
use uuid::Uuid;

/// Its migrations, tolerating ones a newer version already applied (ADR 0072): an older version
/// restarted mid-rollout, or rolled back to, still starts against the schema the newer one
/// expanded.
fn migrator() -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
}

/// Tokens start with this, so a leaked one is easy to recognize.
pub const TOKEN_PREFIX: &str = "awzy_pull_";

/// How long a checked token is trusted without asking the database again. A revoked token
/// stops working within this long.
const CACHE_FOR: Duration = Duration::from_secs(60);

fn token_hash(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub struct TokenStore {
    pool: PgPool,
    cache: Mutex<HashMap<Vec<u8>, (InstallId, Instant)>>,
}

impl TokenStore {
    /// Connects with `schema` first on the search path, creates it if needed, and migrates it.
    pub async fn connect(url: &str, schema: &str) -> anyhow::Result<Self> {
        let valid = schema.starts_with(|c: char| c.is_ascii_lowercase())
            && schema.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        anyhow::ensure!(valid, "bad schema name {schema:?}");
        let options = PgConnectOptions::from_str(url)?.options([("search_path", schema)]);
        let pool = PgPoolOptions::new().max_connections(5).connect_with(options).await?;
        pool.execute(format!("CREATE SCHEMA IF NOT EXISTS {schema}").as_str()).await?;
        migrator().run(&pool).await?;
        Ok(Self { pool, cache: Mutex::new(HashMap::new()) })
    }

    /// Makes a new pull token for `install`, replacing any it had. Returns the token, which
    /// isn't stored.
    pub async fn create(&self, install: InstallId, created_by: &str) -> Result<String, sqlx::Error> {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let token = format!("{TOKEN_PREFIX}{}", URL_SAFE_NO_PAD.encode(secret));
        sqlx::query(
            "INSERT INTO pull_tokens (install_id, token_sha256, created_by) VALUES ($1, $2, $3)
             ON CONFLICT (install_id) DO UPDATE SET token_sha256 = $2, created_by = $3, created_at = now()",
        )
        .bind(install.uuid())
        .bind(token_hash(&token))
        .bind(created_by)
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    /// Removes `install`'s token. Returns whether it had one.
    pub async fn revoke(&self, install: InstallId) -> Result<bool, sqlx::Error> {
        let done = sqlx::query("DELETE FROM pull_tokens WHERE install_id = $1")
            .bind(install.uuid())
            .execute(&self.pool)
            .await?;
        self.cache.lock().unwrap().retain(|_, (id, _)| *id != install);
        Ok(done.rows_affected() > 0)
    }

    /// Whether `token` is `install`'s current pull token.
    pub async fn check(&self, install: InstallId, token: &str) -> Result<bool, sqlx::Error> {
        let hash = token_hash(token);
        if let Some((id, at)) = self.cache.lock().unwrap().get(&hash) {
            if at.elapsed() < CACHE_FOR {
                return Ok(*id == install);
            }
        }
        let id: Option<Uuid> = sqlx::query_scalar("SELECT install_id FROM pull_tokens WHERE token_sha256 = $1")
            .bind(&hash)
            .fetch_optional(&self.pool)
            .await?;
        let Some(id) = id.map(InstallId::from_uuid) else { return Ok(false) };
        self.cache.lock().unwrap().insert(hash, (id, Instant::now()));
        Ok(id == install)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs against AWZY_TEST_POSTGRES_URL in its own schema; skipped without it.
    #[tokio::test]
    async fn create_check_rotate_revoke() {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("AWZY_TEST_POSTGRES_URL is not set; skipping");
            return;
        };
        let schema = format!("test_registry_{}", Uuid::now_v7().simple());
        let store = TokenStore::connect(&url, &schema).await.unwrap();
        let (a, b) = (InstallId::new(), InstallId::new());

        let token = store.create(a, "test").await.unwrap();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert!(store.check(a, &token).await.unwrap());
        assert!(!store.check(b, &token).await.unwrap(), "a token is only good for its own install");
        assert!(!store.check(a, "awzy_pull_wrong").await.unwrap());

        let rotated = store.create(a, "test").await.unwrap();
        assert!(store.check(a, &rotated).await.unwrap());

        assert!(store.revoke(a).await.unwrap());
        assert!(!store.check(a, &rotated).await.unwrap(), "revoking clears the cache too");
    }
}
