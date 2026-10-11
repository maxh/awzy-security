//! Postgres storage for policy bundles.

use std::sync::Arc;

use awzy_model::{EnforcementMode, PolicyBundleId};
use awzy_store::changes::Changes;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

/// Its migrations, tolerating ones a newer version already applied (ADR 0072): an older version
/// restarted mid-rollout, or rolled back to, still starts against the schema the newer one
/// expanded.
fn migrator() -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
}

/// Serializes publishes so two can't both supersede the same active bundle.
const PUBLISH_LOCK: i64 = 0x0061_777a_7970_6f6c; // "awzypol"

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleStatus {
    Draft,
    Active,
    Superseded,
}

impl BundleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
        }
    }

    fn parse(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "draft" => Ok(Self::Draft),
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            other => Err(sqlx::Error::Decode(format!("unknown bundle status {other:?}").into())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PolicyBundle {
    pub id: PolicyBundleId,
    pub version: i64,
    pub schema: String,
    pub policies: String,
    pub status: BundleStatus,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub published_by: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    /// Set when published by a store with a signer.
    pub signature: Option<Vec<u8>>,
    pub signing_key_id: Option<String>,
}

pub use awzy_model::{AUTOMATED_CONTAINMENTS_SCOPE, INSTALL_SCOPE};

/// A scope's enforcement mode and the change that set it.
#[derive(Debug, Clone, PartialEq)]
pub struct EnforcementSetting {
    pub scope: String,
    pub mode: EnforcementMode,
    /// None when the scope has never been switched (and so is in monitor).
    pub changed_by: Option<String>,
    pub changed_at: Option<DateTime<Utc>>,
}

/// A verified break-glass request.
#[derive(Debug, Clone)]
pub struct BreakGlassRequest {
    pub key_id: String,
    pub nonce: String,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    /// None: the bundle published before the active one.
    pub to_version: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct BreakGlassOutcome {
    pub bundle: PolicyBundle,
    pub restored_version: i64,
    pub replaced_version: Option<i64>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("invalid policy bundle: {0}")]
    Invalid(String),
    #[error("policy bundle {0} not found")]
    NotFound(PolicyBundleId),
    #[error("policy bundle {id} is {status}; only drafts can be published")]
    NotDraft { id: PolicyBundleId, status: &'static str },
    #[error("this break-glass request was already used")]
    NonceReused,
    #[error("nothing to roll back to{}", .0.map(|v| format!(": version {v} was never published")).unwrap_or_else(|| ": no earlier published bundle".into()))]
    NoRollbackTarget(Option<i64>),
    #[error("unknown enforcement scope {0:?}; the scopes are \"install\" and \"automated-containments\"")]
    UnknownScope(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

pub struct PolicyStore {
    pool: PgPool,
    signer: Option<awzy_signing::BundleSigner>,
    /// Moves when a bundle is published or the enforcement mode changes (ADR 0077).
    changed: Arc<Changes>,
}

impl PolicyStore {
    /// Publishes unsigned bundles until `with_signer`. PDPs that verify reject those.
    pub fn new(pool: PgPool) -> Self {
        Self { pool, signer: None, changed: Arc::default() }
    }

    /// Follows the Postgres notifications that move [`Self::changed`], forever.
    pub async fn follow_changes(&self) {
        self.changed.clone().follow(self.pool.clone(), "awzy_policy").await
    }

    pub fn changed(&self) -> &Changes {
        &self.changed
    }

    pub fn with_signer(mut self, signer: awzy_signing::BundleSigner) -> Self {
        self.signer = Some(signer);
        self
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        migrator().run(&self.pool).await
    }

    /// Validates the bundle (strict Cedar validation) and stores it as a draft.
    pub async fn create_draft(
        &self,
        schema: &str,
        policies: &str,
        created_by: &str,
    ) -> Result<PolicyBundle, StoreError> {
        awzy_pdp::validate(schema, policies).map_err(|e| StoreError::Invalid(e.to_string()))?;
        let row = sqlx::query(
            "INSERT INTO policy_bundles (id, schema_src, policies_src, status, created_by)
             VALUES ($1, $2, $3, 'draft', $4) RETURNING *",
        )
        .bind(PolicyBundleId::new().uuid())
        .bind(schema)
        .bind(policies)
        .bind(created_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(bundle_from_row(&row)?)
    }

    /// Makes a draft the active bundle. The previously active bundle becomes superseded.
    pub async fn publish(&self, id: PolicyBundleId, published_by: &str) -> Result<PolicyBundle, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(PUBLISH_LOCK).execute(&mut *tx).await?;
        let bundle = self.publish_in_tx(&mut tx, id, published_by).await?;
        tx.commit().await?;
        Ok(bundle)
    }

    /// Publishes inside `tx`, which must already hold the publish lock.
    async fn publish_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        id: PolicyBundleId,
        published_by: &str,
    ) -> Result<PolicyBundle, StoreError> {
        let status: Option<String> = sqlx::query_scalar("SELECT status FROM policy_bundles WHERE id = $1")
            .bind(id.uuid())
            .fetch_optional(&mut **tx)
            .await?;
        match status.as_deref().map(BundleStatus::parse).transpose()? {
            None => return Err(StoreError::NotFound(id)),
            Some(BundleStatus::Draft) => {}
            Some(other) => return Err(StoreError::NotDraft { id, status: other.as_str() }),
        }

        sqlx::query("UPDATE policy_bundles SET status = 'superseded' WHERE status = 'active'")
            .execute(&mut **tx)
            .await?;
        let row = sqlx::query(
            "UPDATE policy_bundles SET status = 'active', published_by = $2, published_at = now()
             WHERE id = $1 RETURNING *",
        )
        .bind(id.uuid())
        .bind(published_by)
        .fetch_one(&mut **tx)
        .await?;
        let mut bundle = bundle_from_row(&row)?;
        if let Some(signer) = &self.signer {
            let signature = signer.sign(&bundle.id.to_string(), bundle.version, &bundle.schema, &bundle.policies);
            sqlx::query("UPDATE policy_bundles SET signature = $2, signing_key_id = $3 WHERE id = $1")
                .bind(id.uuid())
                .bind(&signature)
                .bind(signer.key_id())
                .execute(&mut **tx)
                .await?;
            bundle.signature = Some(signature);
            bundle.signing_key_id = Some(signer.key_id().to_string());
        }
        Ok(bundle)
    }

    /// Break-glass: republishes `to_version` (or, if None, the bundle published before the active
    /// one) as a new active bundle, and records the event. The caller has already verified the
    /// signed request; a reused nonce fails here.
    pub async fn break_glass_rollback(&self, req: &BreakGlassRequest) -> Result<BreakGlassOutcome, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)").bind(PUBLISH_LOCK).execute(&mut *tx).await?;

        let used: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM break_glass_events WHERE nonce = $1)")
            .bind(&req.nonce)
            .fetch_one(&mut *tx)
            .await?;
        if used {
            return Err(StoreError::NonceReused);
        }
        let replaced_version: Option<i64> =
            sqlx::query_scalar("SELECT version FROM policy_bundles WHERE status = 'active'")
                .fetch_optional(&mut *tx)
                .await?;
        let target = match req.to_version {
            Some(v) => sqlx::query("SELECT * FROM policy_bundles WHERE version = $1 AND published_at IS NOT NULL")
                .bind(v)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NoRollbackTarget(Some(v)))?,
            None => sqlx::query(
                "SELECT * FROM policy_bundles WHERE status = 'superseded' AND published_at IS NOT NULL
                 ORDER BY published_at DESC LIMIT 1",
            )
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NoRollbackTarget(None))?,
        };
        let target = bundle_from_row(&target)?;

        let actor = awzy_model::Actor::new(
            awzy_model::EntityRef::new(awzy_model::actor_type::BREAK_GLASS_KEY, req.key_id.clone()),
            None,
        )
        .to_string();
        let copy: Uuid = sqlx::query_scalar(
            "INSERT INTO policy_bundles (id, schema_src, policies_src, status, created_by)
             VALUES ($1, $2, $3, 'draft', $4) RETURNING id",
        )
        .bind(PolicyBundleId::new().uuid())
        .bind(&target.schema)
        .bind(&target.policies)
        .bind(&actor)
        .fetch_one(&mut *tx)
        .await?;
        let bundle = self.publish_in_tx(&mut tx, PolicyBundleId::from_uuid(copy), &actor).await?;

        sqlx::query(
            "INSERT INTO break_glass_events
               (id, key_id, nonce, reason, requested_at, replaced_version, restored_version, new_bundle_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::now_v7())
        .bind(&req.key_id)
        .bind(&req.nonce)
        .bind(&req.reason)
        .bind(req.requested_at)
        .bind(replaced_version)
        .bind(target.version)
        .bind(copy)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(BreakGlassOutcome { bundle, restored_version: target.version, replaced_version })
    }

    pub async fn active(&self) -> Result<Option<PolicyBundle>, StoreError> {
        let row =
            sqlx::query("SELECT * FROM policy_bundles WHERE status = 'active'").fetch_optional(&self.pool).await?;
        Ok(row.as_ref().map(bundle_from_row).transpose()?)
    }

    pub async fn get(&self, id: PolicyBundleId) -> Result<PolicyBundle, StoreError> {
        let row = sqlx::query("SELECT * FROM policy_bundles WHERE id = $1")
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await?;
        Ok(bundle_from_row(&row.ok_or(StoreError::NotFound(id))?)?)
    }

    pub async fn get_by_version(&self, version: i64) -> Result<Option<PolicyBundle>, StoreError> {
        let row = sqlx::query("SELECT * FROM policy_bundles WHERE version = $1")
            .bind(version)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(bundle_from_row).transpose()?)
    }

    pub async fn enforcement(&self, scope: &str) -> Result<EnforcementSetting, StoreError> {
        check_scope(scope)?;
        Ok(self.enforcement_changes(scope, 1).await?.into_iter().next().unwrap_or(EnforcementSetting {
            scope: scope.to_string(),
            mode: EnforcementMode::Monitor,
            changed_by: None,
            changed_at: None,
        }))
    }

    /// Records a switch. Setting the mode a scope already has still records the change.
    pub async fn set_enforcement(
        &self,
        scope: &str,
        mode: EnforcementMode,
        changed_by: &str,
    ) -> Result<EnforcementSetting, StoreError> {
        check_scope(scope)?;
        let row = sqlx::query(
            "INSERT INTO enforcement_mode_changes (id, scope, mode, changed_by) VALUES ($1, $2, $3, $4)
             RETURNING scope, mode, changed_by, changed_at",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(scope)
        .bind(mode.as_str())
        .bind(changed_by)
        .fetch_one(&self.pool)
        .await?;
        Ok(setting_from_row(&row)?)
    }

    /// Newest first.
    pub async fn enforcement_changes(&self, scope: &str, limit: i64) -> Result<Vec<EnforcementSetting>, StoreError> {
        check_scope(scope)?;
        let rows = sqlx::query(
            "SELECT scope, mode, changed_by, changed_at FROM enforcement_mode_changes
             WHERE scope = $1 ORDER BY changed_at DESC, id DESC LIMIT $2",
        )
        .bind(scope)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(setting_from_row).collect::<Result<_, _>>()?)
    }

    /// Newest first.
    pub async fn list(&self, limit: i64) -> Result<Vec<PolicyBundle>, StoreError> {
        let rows = sqlx::query("SELECT * FROM policy_bundles ORDER BY version DESC LIMIT $1")
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(bundle_from_row).collect::<Result<_, _>>()?)
    }
}

fn check_scope(scope: &str) -> Result<(), StoreError> {
    // Policy sets come later.
    if scope == INSTALL_SCOPE || scope == AUTOMATED_CONTAINMENTS_SCOPE {
        Ok(())
    } else {
        Err(StoreError::UnknownScope(scope.to_string()))
    }
}

fn setting_from_row(row: &PgRow) -> Result<EnforcementSetting, sqlx::Error> {
    let mode: String = row.try_get("mode")?;
    Ok(EnforcementSetting {
        scope: row.try_get("scope")?,
        mode: EnforcementMode::parse(&mode)
            .ok_or_else(|| sqlx::Error::Decode(format!("unknown mode {mode:?}").into()))?,
        changed_by: Some(row.try_get("changed_by")?),
        changed_at: Some(row.try_get("changed_at")?),
    })
}

fn bundle_from_row(row: &PgRow) -> Result<PolicyBundle, sqlx::Error> {
    Ok(PolicyBundle {
        id: PolicyBundleId::from_uuid(row.try_get("id")?),
        version: row.try_get("version")?,
        schema: row.try_get("schema_src")?,
        policies: row.try_get("policies_src")?,
        status: BundleStatus::parse(row.try_get("status")?)?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        published_by: row.try_get("published_by")?,
        published_at: row.try_get("published_at")?,
        signature: row.try_get("signature")?,
        signing_key_id: row.try_get("signing_key_id")?,
    })
}

/// Integration tests against Postgres. Run with the local stack up:
///   bazel test --config=local-stack //authz/services/policy:policy_test
/// Without AWZY_TEST_POSTGRES_URL they pass without checking anything, so CI stays green.
#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    use super::*;

    const SCHEMA: &str = awzy_pdp::DEFAULT_SCHEMA;
    const POLICIES: &str = awzy_pdp::DEFAULT_POLICIES;

    /// A store in a fresh Postgres schema, so tests never see each other's rows.
    pub(crate) async fn test_store() -> Option<PolicyStore> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", Uuid::now_v7().simple());
        let admin = PgPool::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        let store = PolicyStore::new(pool);
        store.migrate().await.unwrap();
        Some(store)
    }

    #[tokio::test]
    async fn enforcement_starts_in_monitor_and_keeps_history() {
        let Some(store) = test_store().await else { return };
        let initial = store.enforcement(INSTALL_SCOPE).await.unwrap();
        assert_eq!((initial.mode, initial.changed_by), (EnforcementMode::Monitor, None));

        store.set_enforcement(INSTALL_SCOPE, EnforcementMode::Enforce, "alice").await.unwrap();
        store.set_enforcement(INSTALL_SCOPE, EnforcementMode::Monitor, "bob").await.unwrap();
        let current = store.enforcement(INSTALL_SCOPE).await.unwrap();
        assert_eq!((current.mode, current.changed_by.as_deref()), (EnforcementMode::Monitor, Some("bob")));

        let history: Vec<_> =
            store.enforcement_changes(INSTALL_SCOPE, 10).await.unwrap().into_iter().map(|c| c.mode).collect();
        assert_eq!(history, vec![EnforcementMode::Monitor, EnforcementMode::Enforce]);
        assert!(matches!(store.enforcement("policy_set:x").await.unwrap_err(), StoreError::UnknownScope(_)));
    }

    #[tokio::test]
    async fn automated_containments_have_their_own_mode() {
        let Some(store) = test_store().await else { return };
        let scope = AUTOMATED_CONTAINMENTS_SCOPE;
        assert_eq!(store.enforcement(scope).await.unwrap().mode, EnforcementMode::Monitor, "starts in monitor");
        store.set_enforcement(INSTALL_SCOPE, EnforcementMode::Enforce, "alice").await.unwrap();
        assert_eq!(store.enforcement(scope).await.unwrap().mode, EnforcementMode::Monitor, "apart from the install's");
        store.set_enforcement(scope, EnforcementMode::Enforce, "alice").await.unwrap();
        assert_eq!(store.enforcement(scope).await.unwrap().mode, EnforcementMode::Enforce);
    }

    #[tokio::test]
    async fn published_bundles_are_signed() {
        let Some(store) = test_store().await else { return };
        let key = p256::ecdsa::SigningKey::from_bytes(&[3u8; 32].into()).unwrap();
        let verifier = awzy_signing::BundleVerifier::from_keys([*key.verifying_key()]).unwrap();
        let store = store.with_signer(awzy_signing::BundleSigner::from_key(key).unwrap());

        let draft = store.create_draft(SCHEMA, POLICIES, "alice").await.unwrap();
        assert!(draft.signature.is_none(), "drafts are not signed");
        let published = store.publish(draft.id, "alice").await.unwrap();
        let active = store.active().await.unwrap().unwrap();
        assert_eq!(active.signature, published.signature);
        verifier
            .verify(
                &active.id.to_string(),
                active.version,
                &active.schema,
                &active.policies,
                active.signature.as_deref().unwrap(),
                active.signing_key_id.as_deref().unwrap(),
            )
            .unwrap();
    }

    #[tokio::test]
    async fn break_glass_restores_the_previous_bundle_once_per_nonce() {
        let Some(store) = test_store().await else { return };
        let good = store.create_draft(SCHEMA, POLICIES, "alice").await.unwrap();
        let good = store.publish(good.id, "alice").await.unwrap();
        let bad = store.create_draft(SCHEMA, "forbid (principal, action, resource);", "bob").await.unwrap();
        let bad = store.publish(bad.id, "bob").await.unwrap();

        let req = BreakGlassRequest {
            key_id: "sha256:admin".into(),
            nonce: "n-1".into(),
            reason: "forbid-all locked us out".into(),
            requested_at: chrono::Utc::now(),
            to_version: None,
        };
        let out = store.break_glass_rollback(&req).await.unwrap();
        assert_eq!((out.restored_version, out.replaced_version), (good.version, Some(bad.version)));
        let active = store.active().await.unwrap().unwrap();
        assert_eq!(active.id, out.bundle.id);
        assert_eq!(active.policies, POLICIES);
        assert_eq!(active.published_by.as_deref(), Some("BreakGlassKey::sha256:admin"));

        assert!(matches!(store.break_glass_rollback(&req).await.unwrap_err(), StoreError::NonceReused));
        let missing = BreakGlassRequest { nonce: "n-2".into(), to_version: Some(999), ..req };
        assert!(matches!(
            store.break_glass_rollback(&missing).await.unwrap_err(),
            StoreError::NoRollbackTarget(Some(999))
        ));
    }

    #[tokio::test]
    async fn rejects_invalid_bundles() {
        let Some(store) = test_store().await else { return };
        let err = store
            .create_draft(SCHEMA, r#"permit (principal, action == Action::"delete", resource);"#, "alice")
            .await
            .unwrap_err();
        assert!(matches!(err, StoreError::Invalid(_)), "{err}");
        assert!(store.list(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn publishing_supersedes_the_active_bundle() {
        let Some(store) = test_store().await else { return };
        assert!(store.active().await.unwrap().is_none());

        let first = store.create_draft(SCHEMA, POLICIES, "alice").await.unwrap();
        assert_eq!(first.status, BundleStatus::Draft);
        let first = store.publish(first.id, "alice").await.unwrap();
        assert_eq!(first.status, BundleStatus::Active);
        assert_eq!(first.published_by.as_deref(), Some("alice"));

        let second = store.create_draft(SCHEMA, POLICIES, "bob").await.unwrap();
        assert!(second.version > first.version);
        store.publish(second.id, "bob").await.unwrap();

        let active = store.active().await.unwrap().unwrap();
        assert_eq!(active.id, second.id);
        assert_eq!(store.get(first.id).await.unwrap().version, first.version);
        assert_eq!(store.get_by_version(second.version).await.unwrap().unwrap().id, second.id);
        assert!(store.get_by_version(-1).await.unwrap().is_none());
        let statuses: Vec<_> = store.list(10).await.unwrap().iter().map(|b| b.status).collect();
        assert_eq!(statuses, vec![BundleStatus::Active, BundleStatus::Superseded]);

        // A superseded bundle can't be published again; roll back by publishing a new draft.
        let err = store.publish(first.id, "alice").await.unwrap_err();
        assert!(matches!(err, StoreError::NotDraft { status: "superseded", .. }), "{err}");
        let err = store.publish(PolicyBundleId::new(), "alice").await.unwrap_err();
        assert!(matches!(err, StoreError::NotFound(_)), "{err}");
    }
}
