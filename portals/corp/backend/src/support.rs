//! Support sessions from awzy's side (ADR 0095): staff ask to view named areas of an install's
//! client portal, the request reaches the install in a heartbeat's answer, a client admin decides
//! there, and the install's heartbeats say where the session stands. Once it's approved, the staff
//! member who asked gets a token, signed here, that the client portal accepts until it ends.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject, ID};
use awzy_heartbeat::{SupportRequest, SupportSessionState, SupportStatus};
use awzy_model::{InstallId, SupportSessionId};
use awzy_portal::UtcTimestamp;
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Serialize;
use sqlx::{postgres::PgRow, Row};

use crate::{auth::Staff, fleet::FleetStore};

/// The client portal areas staff may ask for.
pub const AREAS: &[&str] = &["decisions", "policies", "enforcement", "directory", "challenges", "investigations"];

/// The longest session anyone may ask for, as the install allows.
pub const MAX_MINUTES: u32 = 8 * 60;

/// Requests the install hasn't reported after a day are no longer sent.
const PENDING_FOR_HOURS: i64 = 24;

/// One session, as the corp portal knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: SupportSessionId,
    pub install: InstallId,
    pub requested_by: String,
    pub areas: Vec<String>,
    pub reason: String,
    pub duration_minutes: u32,
    pub portal_url: String,
    pub requested_at: DateTime<Utc>,
    pub status: SupportStatus,
    pub expires_at: Option<DateTime<Utc>>,
    /// When the install last said where it stands; None until it has the request.
    pub reported_at: Option<DateTime<Utc>>,
}

#[derive(Debug, thiserror::Error)]
pub enum SupportError {
    #[error("{0}")]
    Invalid(String),
    #[error("support session {0} not found")]
    NotFound(SupportSessionId),
    #[error("{0}")]
    NotYours(String),
    #[error("support tokens aren't set up here (AWZY_CORP_PORTAL_SUPPORT_SIGNING_KEY)")]
    NoSigner,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Token(#[from] jsonwebtoken::errors::Error),
}

const COLUMNS: &str = "id, install_id, requested_by, areas, reason, duration_minutes, portal_url, requested_at, \
                       status, expires_at, reported_at";

fn session_from_row(row: &PgRow) -> Result<Session, sqlx::Error> {
    let status: String = row.try_get("status")?;
    let minutes: i32 = row.try_get("duration_minutes")?;
    Ok(Session {
        id: SupportSessionId::from_uuid(row.try_get("id")?),
        install: InstallId::from_uuid(row.try_get("install_id")?),
        requested_by: row.try_get("requested_by")?,
        areas: row.try_get("areas")?,
        reason: row.try_get("reason")?,
        duration_minutes: minutes.max(0) as u32,
        portal_url: row.try_get("portal_url")?,
        requested_at: row.try_get("requested_at")?,
        status: SupportStatus::parse(&status)
            .ok_or_else(|| sqlx::Error::Decode(format!("unknown support status {status:?}").into()))?,
        expires_at: row.try_get("expires_at")?,
        reported_at: row.try_get("reported_at")?,
    })
}

/// What staff enter to ask for a session.
#[derive(Debug, Clone)]
pub struct NewRequest {
    pub install: InstallId,
    pub areas: Vec<String>,
    pub reason: String,
    pub duration_minutes: u32,
    /// The client portal's URL, where the staff member signs in once it's approved.
    pub portal_url: String,
}

impl NewRequest {
    fn validate(&self) -> Result<(), SupportError> {
        let invalid = |m: &str| Err(SupportError::Invalid(m.to_string()));
        if self.areas.is_empty() || !self.areas.iter().all(|a| AREAS.contains(&a.as_str())) {
            return invalid("ask for one or more client portal areas");
        }
        if self.reason.trim().is_empty() || self.reason.chars().count() > 500 {
            return invalid("say why, in at most 500 characters");
        }
        if self.duration_minutes == 0 || self.duration_minutes > MAX_MINUTES {
            return invalid("a session lasts 1 to 480 minutes");
        }
        if !self.portal_url.starts_with("https://") || self.portal_url.contains(char::is_whitespace) {
            return invalid("the client portal's URL starts with https://");
        }
        Ok(())
    }
}

impl FleetStore {
    pub async fn request_support(&self, new: &NewRequest, by: &str) -> Result<Session, SupportError> {
        new.validate()?;
        let row = sqlx::query(&format!(
            "INSERT INTO support_sessions (id, install_id, requested_by, areas, reason, duration_minutes, portal_url)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING {COLUMNS}"
        ))
        .bind(SupportSessionId::new().uuid())
        .bind(new.install.uuid())
        .bind(by)
        .bind(&new.areas)
        .bind(new.reason.trim())
        .bind(new.duration_minutes as i32)
        .bind(new.portal_url.trim_end_matches('/'))
        .fetch_one(self.pool())
        .await?;
        Ok(session_from_row(&row)?)
    }

    /// An install's sessions, newest first.
    pub async fn support_sessions(&self, install: InstallId) -> Result<Vec<Session>, SupportError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM support_sessions WHERE install_id = $1 ORDER BY requested_at DESC LIMIT 50"
        ))
        .bind(install.uuid())
        .fetch_all(self.pool())
        .await?;
        Ok(rows.iter().map(session_from_row).collect::<Result<_, _>>()?)
    }

    pub async fn support_session(&self, id: SupportSessionId) -> Result<Session, SupportError> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM support_sessions WHERE id = $1"))
            .bind(id.uuid())
            .fetch_optional(self.pool())
            .await?
            .ok_or(SupportError::NotFound(id))?;
        Ok(session_from_row(&row)?)
    }

    /// What a heartbeat says of `install`'s sessions. A state for a session of another install is
    /// ignored.
    pub async fn record_support_states(
        &self,
        install: InstallId,
        states: &[SupportSessionState],
    ) -> Result<(), SupportError> {
        for s in states {
            sqlx::query(
                "UPDATE support_sessions SET status = $3, expires_at = $4, reported_at = now()
                 WHERE id = $1 AND install_id = $2",
            )
            .bind(s.id.uuid())
            .bind(install.uuid())
            .bind(s.status.as_str())
            .bind(s.expires_at)
            .execute(self.pool())
            .await?;
        }
        Ok(())
    }

    /// Requests to send `install` in a heartbeat's answer: those it hasn't reported yet.
    pub async fn pending_support(&self, install: InstallId) -> Result<Vec<SupportRequest>, SupportError> {
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM support_sessions
             WHERE install_id = $1 AND reported_at IS NULL AND requested_at > now() - make_interval(hours => $2)
             ORDER BY requested_at"
        ))
        .bind(install.uuid())
        .bind(PENDING_FOR_HOURS as i32)
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                let s = session_from_row(row)?;
                Ok(SupportRequest {
                    id: s.id,
                    requested_by: s.requested_by,
                    areas: s.areas,
                    reason: s.reason,
                    duration_minutes: s.duration_minutes,
                    requested_at: s.requested_at,
                })
            })
            .collect()
    }
}

// ---- Tokens

/// Who signs support tokens, as the client portal checks it.
pub const ISSUER: &str = "awzy-corp-portal";

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'static str,
    aud: String,
    sub: &'a str,
    sid: String,
    iat: i64,
    exp: i64,
}

/// Signs support tokens with the corp portal's support key (ES256), whose public half every
/// install's client portal trusts.
pub struct SupportSigner {
    key: EncodingKey,
}

impl SupportSigner {
    /// From AWZY_CORP_PORTAL_SUPPORT_SIGNING_KEY: a PKCS#8 PEM P-256 private key. None without it.
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(pem) = std::env::var("AWZY_CORP_PORTAL_SUPPORT_SIGNING_KEY") else { return Ok(None) };
        if pem.trim().is_empty() || pem.trim() == "unset" {
            return Ok(None);
        }
        Ok(Some(Self::from_pem(pem.as_bytes())?))
    }

    pub fn from_pem(pem: &[u8]) -> anyhow::Result<Self> {
        Ok(Self { key: EncodingKey::from_ec_pem(pem)? })
    }

    /// A token for `session`, for the staff member who asked, valid until it expires.
    pub fn sign(&self, session: &Session, staff: &str, now: DateTime<Utc>) -> Result<String, SupportError> {
        if session.requested_by != staff {
            return Err(SupportError::NotYours(format!(
                "support session {} is {}'s",
                session.id, session.requested_by
            )));
        }
        let expires_at = match (session.status, session.expires_at) {
            (SupportStatus::Approved, Some(t)) if t > now => t,
            _ => {
                return Err(SupportError::NotYours(format!(
                    "support session {} isn't approved now ({})",
                    session.id,
                    session.status.as_str()
                )))
            }
        };
        let claims = Claims {
            iss: ISSUER,
            aud: format!("awzy-support:{}", session.install),
            sub: staff,
            sid: session.id.to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
        };
        Ok(jsonwebtoken::encode(&Header::new(Algorithm::ES256), &claims, &self.key)?)
    }
}

// ---- GraphQL

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(name = "SupportSessionStatus")]
pub enum Status {
    /// Asked for; the install hasn't decided, or hasn't got it yet.
    Requested,
    Approved,
    Rejected,
    Ended,
    Expired,
}

impl From<SupportStatus> for Status {
    fn from(s: SupportStatus) -> Self {
        match s {
            SupportStatus::Requested => Self::Requested,
            SupportStatus::Approved => Self::Approved,
            SupportStatus::Rejected => Self::Rejected,
            SupportStatus::Ended => Self::Ended,
            SupportStatus::Expired => Self::Expired,
        }
    }
}

pub struct SupportSession(pub Session);

#[Object]
impl SupportSession {
    pub async fn id(&self) -> ID {
        ID(self.0.id.to_string())
    }

    async fn status(&self) -> Status {
        self.0.status.into()
    }

    async fn requested_by(&self) -> &str {
        &self.0.requested_by
    }

    async fn areas(&self) -> &[String] {
        &self.0.areas
    }

    async fn reason(&self) -> &str {
        &self.0.reason
    }

    async fn duration_minutes(&self) -> u32 {
        self.0.duration_minutes
    }

    async fn portal_url(&self) -> &str {
        &self.0.portal_url
    }

    async fn requested_at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.requested_at)
    }

    async fn expires_at(&self) -> Option<UtcTimestamp> {
        self.0.expires_at.map(UtcTimestamp)
    }

    /// Whether the install has the request yet.
    async fn delivered(&self) -> bool {
        self.0.reported_at.is_some()
    }
}

#[derive(InputObject)]
pub struct RequestSupportSessionInput {
    pub install_qid: ID,
    /// Client portal areas, e.g. "decisions".
    pub areas: Vec<String>,
    pub reason: String,
    pub duration_minutes: u32,
    /// The client portal's URL, e.g. https://portal.example.com.
    pub portal_url: String,
}

#[derive(SimpleObject)]
pub struct SupportSignIn {
    /// Opens the client portal signed in to the session. Holds the token: don't share it.
    pub sign_in_url: String,
    pub expires_at: UtcTimestamp,
}

fn store<'a>(ctx: &Context<'a>) -> Result<&'a FleetStore> {
    ctx.data::<FleetStore>().map_err(|_| "Postgres is not configured (AWZY_POSTGRES_URL)".into())
}

#[derive(Default)]
pub struct SupportMutation;

#[Object]
impl SupportMutation {
    /// Asks an install's admins for a support session. It reaches the install with its next
    /// heartbeat; they approve or reject it in their client portal.
    async fn request_support_session(
        &self,
        ctx: &Context<'_>,
        input: RequestSupportSessionInput,
    ) -> Result<SupportSession> {
        let staff = ctx.data::<Staff>()?;
        let install: InstallId = input.install_qid.parse()?;
        let new = NewRequest {
            install,
            areas: input.areas,
            reason: input.reason,
            duration_minutes: input.duration_minutes,
            portal_url: input.portal_url,
        };
        let session = store(ctx)?.request_support(&new, &staff.username).await?;
        tracing::info!(session = %session.id, %install, by = %staff.username, areas = ?session.areas, "support session requested");
        Ok(SupportSession(session))
    }

    /// Signs the staff member who asked in to an approved session's client portal.
    async fn sign_in_to_support_session(&self, ctx: &Context<'_>, support_session_qid: ID) -> Result<SupportSignIn> {
        let staff = ctx.data::<Staff>()?;
        let store = store(ctx)?;
        let signer = store.support_signer.clone().ok_or(SupportError::NoSigner)?;
        let session = store.support_session(support_session_qid.parse()?).await?;
        let token = signer.sign(&session, &staff.username, Utc::now())?;
        tracing::info!(session = %session.id, install = %session.install, by = %staff.username, "support session token issued");
        Ok(SupportSignIn {
            sign_in_url: format!("{}/support/sign-in#token={token}", session.portal_url),
            expires_at: UtcTimestamp(session.expires_at.unwrap_or_else(Utc::now)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(status: SupportStatus, minutes_left: i64) -> Session {
        Session {
            id: SupportSessionId::new(),
            install: InstallId::new(),
            requested_by: "max".into(),
            areas: vec!["decisions".into()],
            reason: "latency".into(),
            duration_minutes: 60,
            portal_url: "https://portal.example.com".into(),
            requested_at: Utc::now(),
            status,
            expires_at: Some(Utc::now() + chrono::Duration::minutes(minutes_left)),
            reported_at: Some(Utc::now()),
        }
    }

    fn signer() -> SupportSigner {
        use p256::pkcs8::{EncodePrivateKey, LineEnding};
        let key = p256::ecdsa::SigningKey::from_bytes(&[9u8; 32].into()).unwrap();
        SupportSigner::from_pem(key.to_pkcs8_pem(LineEnding::LF).unwrap().as_bytes()).unwrap()
    }

    #[test]
    fn a_token_is_only_for_the_staff_member_who_asked_while_approved() {
        let s = signer();
        let now = Utc::now();
        let ok = session(SupportStatus::Approved, 30);
        let token = s.sign(&ok, "max", now).unwrap();
        let claims: serde_json::Value = {
            use base64::Engine;
            let payload = token.split('.').nth(1).unwrap();
            serde_json::from_slice(&base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
        };
        assert_eq!(claims["aud"], format!("awzy-support:{}", ok.install));
        assert_eq!(claims["sid"], ok.id.to_string());
        assert_eq!(claims["exp"], ok.expires_at.unwrap().timestamp());
        assert!(s.sign(&ok, "someone-else", now).is_err());
        assert!(s.sign(&session(SupportStatus::Requested, 30), "max", now).is_err());
        assert!(s.sign(&session(SupportStatus::Approved, -1), "max", now).is_err(), "expired");
    }

    #[test]
    fn a_request_names_known_areas_a_reason_a_duration_and_the_portal() {
        let base = NewRequest {
            install: InstallId::new(),
            areas: vec!["decisions".into()],
            reason: "latency".into(),
            duration_minutes: 60,
            portal_url: "https://portal.example.com".into(),
        };
        assert!(base.validate().is_ok());
        assert!(NewRequest { areas: vec!["everything".into()], ..base.clone() }.validate().is_err());
        assert!(NewRequest { duration_minutes: MAX_MINUTES + 1, ..base.clone() }.validate().is_err());
        assert!(NewRequest { portal_url: "http://portal".into(), ..base.clone() }.validate().is_err());
        assert!(NewRequest { reason: " ".into(), ..base }.validate().is_err());
    }
}
