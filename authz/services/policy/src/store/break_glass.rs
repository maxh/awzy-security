//! The record of break-glass rollbacks, for the portal and auditors (#324). Each is written by
//! [`PolicyStore::break_glass_rollback`] in the same transaction as the bundle it publishes.

use awzy_model::PolicyBundleId;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use super::{PolicyStore, StoreError};

/// One break-glass rollback, as recorded when it was made.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakGlassEvent {
    /// The enrolled key that signed it: `sha256:<fingerprint>`.
    pub key_id: String,
    pub reason: String,
    /// When the signed request says it was made.
    pub requested_at: DateTime<Utc>,
    /// When awzy-policy carried it out.
    pub recorded_at: DateTime<Utc>,
    pub restored_version: i64,
    /// The version active before; None if none was.
    pub replaced_version: Option<i64>,
    /// The bundle it published: a copy of the restored version.
    pub bundle_id: PolicyBundleId,
}

impl PolicyStore {
    /// Every break-glass rollback, newest first, at most `limit`.
    pub async fn break_glass_events(&self, limit: i64) -> Result<Vec<BreakGlassEvent>, StoreError> {
        let rows = sqlx::query(
            "SELECT key_id, reason, requested_at, created_at, restored_version, replaced_version, new_bundle_id
             FROM break_glass_events ORDER BY created_at DESC, id DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(BreakGlassEvent {
                    key_id: row.try_get("key_id")?,
                    reason: row.try_get("reason")?,
                    requested_at: row.try_get("requested_at")?,
                    recorded_at: row.try_get("created_at")?,
                    restored_version: row.try_get("restored_version")?,
                    replaced_version: row.try_get("replaced_version")?,
                    bundle_id: PolicyBundleId::from_uuid(row.try_get::<Uuid, _>("new_bundle_id")?),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{store::tests::test_store, BreakGlassRequest};

    #[tokio::test]
    async fn each_rollback_is_listed_with_its_key_reason_and_bundle() {
        let Some(store) = test_store().await else { return };
        let good = store.create_draft(awzy_pdp::DEFAULT_SCHEMA, awzy_pdp::DEFAULT_POLICIES, "alice").await.unwrap();
        let good = store.publish(good.id, "alice").await.unwrap();
        let bad =
            store.create_draft(awzy_pdp::DEFAULT_SCHEMA, "forbid (principal, action, resource);", "bob").await.unwrap();
        let bad = store.publish(bad.id, "bob").await.unwrap();
        assert!(store.break_glass_events(50).await.unwrap().is_empty());

        let req = BreakGlassRequest {
            key_id: "sha256:admin".into(),
            nonce: "n-1".into(),
            reason: "forbid-all locked us out".into(),
            requested_at: chrono::Utc::now(),
            to_version: None,
        };
        let out = store.break_glass_rollback(&req).await.unwrap();
        let listed = store.break_glass_events(50).await.unwrap();
        assert_eq!(listed.len(), 1);
        let event = &listed[0];
        assert_eq!(
            (event.key_id.as_str(), event.reason.as_str(), event.restored_version, event.replaced_version),
            ("sha256:admin", "forbid-all locked us out", good.version, Some(bad.version))
        );
        assert_eq!(event.bundle_id, out.bundle.id);
    }
}
