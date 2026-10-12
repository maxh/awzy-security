//! The model routing setting (ADR 0114): which models Awzy Bot and investigations use, kept and
//! signed like the outage fallback. awzy-policy holds it as the JSON the portal checked against
//! the install's connections; it only makes sure it's a JSON object.

use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, Row};

use super::{PolicyStore, StoreError};

/// One version of the model routing. Version 0, with no one and no time, is the setting before
/// anyone made one: each service keeps the routing it started with.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelRoutingSetting {
    pub version: i64,
    pub routing_json: String,
    pub changed_by: Option<String>,
    pub changed_at: Option<DateTime<Utc>>,
    pub signature: Option<Vec<u8>>,
    pub signing_key_id: Option<String>,
}

const COLUMNS: &str = "version, routing_json, changed_by, changed_at, signature, signing_key_id";

impl PolicyStore {
    pub async fn model_routing(&self) -> Result<ModelRoutingSetting, StoreError> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM model_routing_changes ORDER BY version DESC LIMIT 1"))
            .fetch_optional(&self.pool)
            .await?;
        Ok(match row {
            Some(row) => from_row(&row)?,
            None => ModelRoutingSetting {
                version: 0,
                routing_json: "{}".into(),
                changed_by: None,
                changed_at: None,
                signature: None,
                signing_key_id: None,
            },
        })
    }

    /// Records a new version, signed when the store has a signer. Two changes at once can't take
    /// the same version: the second fails, to be made again.
    pub async fn set_model_routing(
        &self,
        routing_json: &str,
        changed_by: &str,
    ) -> Result<ModelRoutingSetting, StoreError> {
        match serde_json::from_str::<serde_json::Value>(routing_json) {
            Ok(serde_json::Value::Object(_)) => {}
            _ => return Err(StoreError::Invalid("a model routing is a JSON object".into())),
        }
        let mut tx = self.pool.begin().await?;
        // Whole seconds, as signed, so the stored time and the signature agree.
        let (version, changed_at): (i64, DateTime<Utc>) = sqlx::query_as(
            "SELECT coalesce(max(version), 0) + 1, date_trunc('second', now()) FROM model_routing_changes",
        )
        .fetch_one(&mut *tx)
        .await?;
        let (signature, key_id) = match &self.signer {
            Some(signer) => {
                let payload =
                    awzy_signing::model_routing_payload(version, changed_by, changed_at.timestamp(), routing_json);
                (Some(signer.sign_payload(&payload).await?), Some(signer.key_id().to_string()))
            }
            None => (None, None),
        };
        let row = sqlx::query(&format!(
            "INSERT INTO model_routing_changes
               (version, routing_json, changed_by, changed_at, signature, signing_key_id)
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING {COLUMNS}"
        ))
        .bind(version)
        .bind(routing_json)
        .bind(changed_by)
        .bind(changed_at)
        .bind(signature)
        .bind(key_id)
        .fetch_one(&mut *tx)
        .await?;
        let setting = from_row(&row)?;
        tx.commit().await?;
        Ok(setting)
    }
}

fn from_row(row: &PgRow) -> Result<ModelRoutingSetting, sqlx::Error> {
    Ok(ModelRoutingSetting {
        version: row.try_get("version")?,
        routing_json: row.try_get("routing_json")?,
        changed_by: Some(row.try_get("changed_by")?),
        changed_at: Some(row.try_get("changed_at")?),
        signature: row.try_get("signature")?,
        signing_key_id: row.try_get("signing_key_id")?,
    })
}
