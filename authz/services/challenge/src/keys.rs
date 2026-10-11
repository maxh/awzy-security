//! Security keys (ADR 0059): enrolling them, and passing verification challenges with them.
//!
//! - A person enrolls their first key on the strength of their IdP sign-in alone. Adding
//!   another, or removing one, needs a fresh assertion from a key they already have, so a
//!   stolen session can't swap in the thief's key. An admin can reset a lost key.
//! - A verification challenge passes on an assertion from one of the answerer's keys, over a
//!   ceremony issued for that challenge.
//! - Every ceremony is single-use, bound to one person and purpose, and expires after five
//!   minutes. It's spent before the response is checked, so a failed attempt can't be retried
//!   against the same challenge.

use std::{sync::Arc, time::Duration};

use awzy_model::{CeremonyId, ChallengeId, ChallengeKind, SecurityKeyId};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};

use crate::{
    store::{Challenge, ChallengeStore, StoreError},
    webauthn::{RelyingParty, WebAuthnError, ES256},
};

/// How long a browser has to finish a ceremony.
pub const CEREMONY_LIFETIME: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq)]
pub struct SecurityKey {
    pub id: SecurityKeyId,
    pub human_id: String,
    pub name: String,
    /// The key that authorized adding this one; None for a person's first key.
    pub added_with: Option<SecurityKeyId>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub removed_at: Option<DateTime<Utc>>,
    pub removed_by: Option<String>,
}

/// What a browser needs for one ceremony: its ID, and the options for
/// `navigator.credentials.create()` or `.get()`, in WebAuthn's JSON form (binary fields
/// base64url).
#[derive(Debug, Clone)]
pub struct Ceremony {
    pub id: CeremonyId,
    pub options: Value,
}

/// A browser's `navigator.credentials.get()` response.
#[derive(Debug, Clone, Default)]
pub struct Assertion {
    pub ceremony_id: String,
    pub credential_id: Vec<u8>,
    pub client_data_json: Vec<u8>,
    pub authenticator_data: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("{0}")]
    WebAuthn(#[from] WebAuthnError),
    #[error("{human} has no security key; add one on the Challenges page first")]
    NoKeys { human: String },
    #[error("{human} already has a security key, so a new one must be authorized with it")]
    NeedsExistingKey { human: String },
    #[error("that ceremony is unknown, expired, already used, or for something else; start again")]
    BadCeremony,
    #[error("that security key isn't one of {human}'s")]
    UnknownKey { human: String },
    #[error("security key {0} not found")]
    NotFound(SecurityKeyId),
    #[error("that security key is already registered")]
    AlreadyRegistered,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Challenge(#[from] StoreError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    Register,
    KeyChange,
    Verify,
}

impl Purpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::KeyChange => "key_change",
            Self::Verify => "verify",
        }
    }
}

/// A spent ceremony.
struct Spent {
    challenge: Vec<u8>,
    challenge_id: Option<ChallengeId>,
    authorized_by: Option<SecurityKeyId>,
}

pub struct SecurityKeys {
    pool: PgPool,
    rp: RelyingParty,
    challenges: Arc<ChallengeStore>,
}

fn random_challenge() -> Vec<u8> {
    let mut bytes = vec![0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random number generator");
    bytes
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn credential_descriptors(ids: &[Vec<u8>]) -> Value {
    Value::Array(ids.iter().map(|id| json!({ "type": "public-key", "id": b64(id) })).collect())
}

impl SecurityKeys {
    pub fn new(pool: PgPool, rp: RelyingParty, challenges: Arc<ChallengeStore>) -> Self {
        Self { pool, rp, challenges }
    }

    /// A person's keys in use, oldest first.
    pub async fn list(&self, human: &str) -> Result<Vec<SecurityKey>, KeyError> {
        let rows =
            sqlx::query("SELECT * FROM security_keys WHERE human_id = $1 AND removed_at IS NULL ORDER BY created_at")
                .bind(human)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.iter().map(key_from_row).collect::<Result<_, _>>()?)
    }

    async fn credential_ids(&self, human: &str) -> Result<Vec<Vec<u8>>, KeyError> {
        let rows = sqlx::query(
            "SELECT credential_id FROM security_keys WHERE human_id = $1 AND removed_at IS NULL ORDER BY created_at",
        )
        .bind(human)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(|r| r.try_get("credential_id")).collect::<Result<_, _>>()?)
    }

    async fn issue(
        &self,
        human: &str,
        purpose: Purpose,
        challenge_id: Option<ChallengeId>,
        authorized_by: Option<SecurityKeyId>,
    ) -> Result<(CeremonyId, Vec<u8>), KeyError> {
        let id = CeremonyId::new();
        let challenge = random_challenge();
        sqlx::query(
            "INSERT INTO webauthn_ceremonies (id, human_id, purpose, challenge, challenge_id, authorized_by, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(secs => $7))",
        )
        .bind(id.uuid())
        .bind(human)
        .bind(purpose.as_str())
        .bind(&challenge)
        .bind(challenge_id.map(|c| c.uuid()))
        .bind(authorized_by.map(|k| k.uuid()))
        .bind(CEREMONY_LIFETIME.as_secs_f64())
        .execute(&self.pool)
        .await?;
        Ok((id, challenge))
    }

    /// Marks the ceremony used, committed on its own, before its response is checked.
    async fn spend(&self, id: &str, human: &str, purpose: Purpose) -> Result<Spent, KeyError> {
        let id: CeremonyId = id.parse().map_err(|_| KeyError::BadCeremony)?;
        let row = sqlx::query(
            "UPDATE webauthn_ceremonies SET used_at = now() \
             WHERE id = $1 AND human_id = $2 AND purpose = $3 AND used_at IS NULL AND expires_at > now() \
             RETURNING challenge, challenge_id, authorized_by",
        )
        .bind(id.uuid())
        .bind(human)
        .bind(purpose.as_str())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(KeyError::BadCeremony)?;
        Ok(Spent {
            challenge: row.try_get("challenge")?,
            challenge_id: row.try_get::<Option<uuid::Uuid>, _>("challenge_id")?.map(ChallengeId::from_uuid),
            authorized_by: row.try_get::<Option<uuid::Uuid>, _>("authorized_by")?.map(SecurityKeyId::from_uuid),
        })
    }

    /// Checks `a` against `challenge` with `person`'s key and moves its counter forward, in the
    /// caller's transaction. Returns the key that signed.
    async fn check_assertion(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        human: &str,
        challenge: &[u8],
        a: &Assertion,
    ) -> Result<SecurityKeyId, KeyError> {
        let row = sqlx::query(
            "SELECT id, public_key, sign_count FROM security_keys \
             WHERE credential_id = $1 AND human_id = $2 AND removed_at IS NULL FOR UPDATE",
        )
        .bind(&a.credential_id)
        .bind(human)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| KeyError::UnknownKey { human: human.into() })?;
        let id = SecurityKeyId::from_uuid(row.try_get("id")?);
        let count: i64 = row.try_get("sign_count")?;
        let asserted = self.rp.verify_assertion(
            challenge,
            &row.try_get::<Vec<u8>, _>("public_key")?,
            u32::try_from(count).unwrap_or(u32::MAX),
            &a.client_data_json,
            &a.authenticator_data,
            &a.signature,
        )?;
        sqlx::query("UPDATE security_keys SET sign_count = $2, last_used_at = now() WHERE id = $1")
            .bind(id.uuid())
            .bind(i64::from(asserted.sign_count))
            .execute(&mut **tx)
            .await?;
        Ok(id)
    }

    fn request_options(&self, challenge: &[u8], credentials: &[Vec<u8>]) -> Value {
        json!({
            "challenge": b64(challenge),
            "rpId": self.rp.id(),
            "allowCredentials": credential_descriptors(credentials),
            // A security-key challenge proves the key is present, not who's holding it.
            "userVerification": "discouraged",
            "timeout": CEREMONY_LIFETIME.as_millis() as u64,
        })
    }

    /// Step one of adding or removing a key, for someone who already has one: an assertion
    /// ceremony over their keys.
    pub async fn begin_key_change(&self, human: &str) -> Result<Ceremony, KeyError> {
        let ids = self.credential_ids(human).await?;
        if ids.is_empty() {
            return Err(KeyError::NoKeys { human: human.into() });
        }
        let (id, challenge) = self.issue(human, Purpose::KeyChange, None, None).await?;
        Ok(Ceremony { id, options: self.request_options(&challenge, &ids) })
    }

    /// A registration ceremony. Someone with a key must pass `authorization`, an assertion over
    /// a key-change ceremony.
    pub async fn begin_registration(
        &self,
        human: &str,
        authorization: Option<&Assertion>,
    ) -> Result<Ceremony, KeyError> {
        let ids = self.credential_ids(human).await?;
        let authorized_by = match (ids.is_empty(), authorization) {
            (true, _) => None,
            (false, None) => return Err(KeyError::NeedsExistingKey { human: human.into() }),
            (false, Some(a)) => {
                let spent = self.spend(&a.ceremony_id, human, Purpose::KeyChange).await?;
                let mut tx = self.pool.begin().await?;
                let key = self.check_assertion(&mut tx, human, &spent.challenge, a).await?;
                tx.commit().await?;
                Some(key)
            }
        };
        let (id, challenge) = self.issue(human, Purpose::Register, None, authorized_by).await?;
        let options = json!({
            "challenge": b64(&challenge),
            "rp": { "id": self.rp.id(), "name": "awzy" },
            // Opaque and stable per person, without putting their ID on the key.
            "user": { "id": b64(&Sha256::digest(human.as_bytes())), "name": human, "displayName": human },
            "pubKeyCredParams": [{ "type": "public-key", "alg": ES256 }],
            "excludeCredentials": credential_descriptors(&ids),
            "authenticatorSelection": { "residentKey": "discouraged", "userVerification": "discouraged" },
            "attestation": "none",
            "timeout": CEREMONY_LIFETIME.as_millis() as u64,
        });
        Ok(Ceremony { id, options })
    }

    /// Checks the browser's `create()` response and saves the key.
    pub async fn finish_registration(
        &self,
        human: &str,
        ceremony_id: &str,
        name: &str,
        client_data_json: &[u8],
        attestation_object: &[u8],
    ) -> Result<SecurityKey, KeyError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(KeyError::Invalid("name the key (up to 100 characters)".into()));
        }
        let spent = self.spend(ceremony_id, human, Purpose::Register).await?;
        let cred = self.rp.verify_registration(&spent.challenge, client_data_json, attestation_object)?;
        let mut tx = self.pool.begin().await?;
        // Serializes enrollment per person, so two "first keys" can't both skip authorization.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(human).execute(&mut *tx).await?;
        let has_keys = sqlx::query("SELECT 1 FROM security_keys WHERE human_id = $1 AND removed_at IS NULL LIMIT 1")
            .bind(human)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
        if has_keys && spent.authorized_by.is_none() {
            return Err(KeyError::NeedsExistingKey { human: human.into() });
        }
        let row = sqlx::query(
            "INSERT INTO security_keys (id, human_id, credential_id, public_key, sign_count, name, added_with) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (credential_id) DO NOTHING RETURNING *",
        )
        .bind(SecurityKeyId::new().uuid())
        .bind(human)
        .bind(&cred.credential_id)
        .bind(&cred.public_key)
        .bind(i64::from(cred.sign_count))
        .bind(name)
        .bind(spent.authorized_by.map(|k| k.uuid()))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(KeyError::AlreadyRegistered)?;
        tx.commit().await?;
        Ok(key_from_row(&row)?)
    }

    /// Removes one of `person`'s keys, authorized by an assertion from any of their keys.
    pub async fn remove(
        &self,
        human: &str,
        id: SecurityKeyId,
        authorization: &Assertion,
    ) -> Result<SecurityKey, KeyError> {
        let spent = self.spend(&authorization.ceremony_id, human, Purpose::KeyChange).await?;
        let mut tx = self.pool.begin().await?;
        let with = self.check_assertion(&mut tx, human, &spent.challenge, authorization).await?;
        let row = sqlx::query(
            "UPDATE security_keys SET removed_at = now(), removed_with = $3, removed_by = $2 \
             WHERE id = $1 AND human_id = $2 AND removed_at IS NULL RETURNING *",
        )
        .bind(id.uuid())
        .bind(human)
        .bind(with.uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(KeyError::NotFound(id))?;
        tx.commit().await?;
        Ok(key_from_row(&row)?)
    }

    /// An admin removes a key, for someone who lost it. They can then enroll a first key again.
    pub async fn reset(&self, id: SecurityKeyId, removed_by: &str) -> Result<SecurityKey, KeyError> {
        if removed_by.is_empty() {
            return Err(KeyError::Invalid("removed_by is required".into()));
        }
        let row = sqlx::query(
            "UPDATE security_keys SET removed_at = now(), removed_by = $2 WHERE id = $1 AND removed_at IS NULL RETURNING *",
        )
        .bind(id.uuid())
        .bind(removed_by)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(KeyError::NotFound(id))?;
        Ok(key_from_row(&row)?)
    }

    /// An assertion ceremony for verification challenge `challenge_id`, which `person` must be
    /// the one to answer.
    pub async fn begin_verification(&self, human: &str, challenge_id: ChallengeId) -> Result<Ceremony, KeyError> {
        // Checked now so the person learns at once; checked again, locked, when they finish.
        let mut tx = self.challenges.begin().await?;
        self.challenges.lock_for(&mut tx, challenge_id, human, ChallengeKind::Verification).await?;
        tx.rollback().await?;
        let ids = self.credential_ids(human).await?;
        if ids.is_empty() {
            return Err(KeyError::NoKeys { human: human.into() });
        }
        let (id, challenge) = self.issue(human, Purpose::Verify, Some(challenge_id), None).await?;
        Ok(Ceremony { id, options: self.request_options(&challenge, &ids) })
    }

    /// Passes the verification on the ceremony's challenge with the browser's `get()` response.
    pub async fn finish_verification(&self, human: &str, a: &Assertion) -> Result<Challenge, KeyError> {
        let spent = self.spend(&a.ceremony_id, human, Purpose::Verify).await?;
        let challenge_id = spent.challenge_id.ok_or(KeyError::BadCeremony)?;
        let mut tx = self.challenges.begin().await?;
        let c = self.challenges.lock_for(&mut tx, challenge_id, human, ChallengeKind::Verification).await?;
        self.check_assertion(&mut tx, human, &spent.challenge, a).await?;
        let c = self.challenges.mark_passed(&mut tx, &c, ChallengeKind::Verification).await?;
        tx.commit().await?;
        Ok(c)
    }
}

fn key_from_row(row: &PgRow) -> Result<SecurityKey, sqlx::Error> {
    Ok(SecurityKey {
        id: SecurityKeyId::from_uuid(row.try_get("id")?),
        human_id: row.try_get("human_id")?,
        name: row.try_get("name")?,
        added_with: row.try_get::<Option<uuid::Uuid>, _>("added_with")?.map(SecurityKeyId::from_uuid),
        created_at: row.try_get("created_at")?,
        last_used_at: row.try_get("last_used_at")?,
        removed_at: row.try_get("removed_at")?,
        removed_by: row.try_get("removed_by")?,
    })
}

/// Against Postgres, with software authenticators (see `webauthn::testing`).
#[cfg(test)]
mod tests {
    use awzy_model::{Challenges, DecisionId, EntityRef};

    use super::*;
    use crate::{
        store::{tests::test_store, ChallengeStatus, Lifetimes, RequestKey},
        webauthn::testing::{SoftKey, PRESENT},
    };

    const RP: &str = "portal.example.com";
    const ORIGIN: &str = "https://portal.example.com";

    async fn setup() -> Option<(SecurityKeys, Arc<ChallengeStore>)> {
        let store = Arc::new(test_store(Lifetimes::default()).await?);
        let keys = SecurityKeys::new(store.pool().clone(), RelyingParty::new(RP, [ORIGIN]), store.clone());
        Some((keys, store))
    }

    fn challenge_of(c: &Ceremony) -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(c.options["challenge"].as_str().unwrap()).unwrap()
    }

    /// Runs `key` through a get() ceremony.
    fn assertion(key: &mut SoftKey, ceremony: &Ceremony) -> Assertion {
        let (client, auth, sig) = key.assert(&challenge_of(ceremony), PRESENT);
        Assertion {
            ceremony_id: ceremony.id.to_string(),
            credential_id: key.credential_id.clone(),
            client_data_json: client,
            authenticator_data: auth,
            signature: sig,
        }
    }

    async fn enroll(
        keys: &SecurityKeys,
        human: &str,
        key: &SoftKey,
        authorization: Option<&Assertion>,
    ) -> Result<SecurityKey, KeyError> {
        let ceremony = keys.begin_registration(human, authorization).await?;
        let (client, object) = key.register(&challenge_of(&ceremony));
        keys.finish_registration(human, &ceremony.id.to_string(), "YubiKey", &client, &object).await
    }

    #[tokio::test]
    async fn the_first_key_enrolls_and_later_ones_need_it() {
        let Some((keys, _)) = setup().await else { return };
        let mut first = SoftKey::new(1, RP, ORIGIN);
        let k1 = enroll(&keys, "bob", &first, None).await.unwrap();
        assert_eq!(k1.added_with, None);

        let second = SoftKey::new(2, RP, ORIGIN);
        assert!(matches!(enroll(&keys, "bob", &second, None).await, Err(KeyError::NeedsExistingKey { .. })));

        let change = keys.begin_key_change("bob").await.unwrap();
        let auth = assertion(&mut first, &change);
        let k2 = enroll(&keys, "bob", &second, Some(&auth)).await.unwrap();
        assert_eq!(k2.added_with, Some(k1.id));
        assert!(matches!(enroll(&keys, "bob", &second, Some(&auth)).await, Err(KeyError::BadCeremony)), "spent");

        // Someone else's key can't authorize bob's changes.
        let mut mallory = SoftKey::new(3, RP, ORIGIN);
        enroll(&keys, "mallory", &mallory, None).await.unwrap();
        let change = keys.begin_key_change("bob").await.unwrap();
        let forged = assertion(&mut mallory, &change);
        assert!(matches!(enroll(&keys, "bob", &mallory, Some(&forged)).await, Err(KeyError::UnknownKey { .. })));

        // Removing needs a key too; an admin reset doesn't.
        let change = keys.begin_key_change("bob").await.unwrap();
        let removed = keys.remove("bob", k2.id, &assertion(&mut first, &change)).await.unwrap();
        assert_eq!(removed.removed_by.as_deref(), Some("bob"));
        keys.reset(k1.id, "alice").await.unwrap();
        assert!(keys.list("bob").await.unwrap().is_empty());
        enroll(&keys, "bob", &SoftKey::new(4, RP, ORIGIN), None).await.expect("a first key again after the reset");
    }

    #[tokio::test]
    async fn a_security_key_passes_the_verification_it_was_asked_for() {
        let Some((keys, store)) = setup().await else { return };
        let request = RequestKey {
            principal: EntityRef::new("Human", "bob"),
            on_behalf_of: None,
            action: "read".into(),
            resource: EntityRef::new("Resource", "board-deck"),
        };
        let required = Challenges::of(ChallengeKind::Verification);
        let id = store.resolve(DecisionId::new(), &request, required, Some("bob"), true).await.unwrap();
        let id = id.challenge_id.unwrap();

        assert!(matches!(keys.begin_verification("bob", id).await, Err(KeyError::NoKeys { .. })));
        let mut key = SoftKey::new(1, RP, ORIGIN);
        enroll(&keys, "bob", &key, None).await.unwrap();
        let mut carols = SoftKey::new(2, RP, ORIGIN);
        enroll(&keys, "carol", &carols, None).await.unwrap();
        assert!(matches!(
            keys.begin_verification("carol", id).await,
            Err(KeyError::Challenge(StoreError::NotAllowed(_)))
        ));

        // A response for one ceremony can't answer another, and a failed attempt spends it.
        let ceremony = keys.begin_verification("bob", id).await.unwrap();
        let mut wrong = assertion(&mut carols, &ceremony);
        wrong.credential_id = key.credential_id.clone();
        assert!(matches!(
            keys.finish_verification("bob", &wrong).await,
            Err(KeyError::WebAuthn(WebAuthnError::BadSignature))
        ));
        let retry = assertion(&mut key, &ceremony);
        assert!(matches!(keys.finish_verification("bob", &retry).await, Err(KeyError::BadCeremony)));

        let ceremony = keys.begin_verification("bob", id).await.unwrap();
        let c = keys.finish_verification("bob", &assertion(&mut key, &ceremony)).await.unwrap();
        assert_eq!((c.status, c.passed), (ChallengeStatus::Passed, required));
        let r = store.resolve(DecisionId::new(), &request, required, Some("bob"), true).await.unwrap();
        assert_eq!(r.passed, required, "the retried request is allowed");
        assert!(keys.list("bob").await.unwrap()[0].last_used_at.is_some());
    }
}
