//! Support sessions (ADR 0095): awzy staff ask, in the corp portal, to view named areas of this
//! install's client portal; awzy-reporter brings the request in; a client admin approves or
//! rejects it here, and can end it early. While an approved session lasts, the client portal lets
//! the staff member view the approved areas, read-only, and records each operation.

use chrono::{DateTime, Duration, Utc};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

use awzy_model::SupportSessionId;

/// The longest session anyone may ask for.
pub const MAX_MINUTES: u32 = 8 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Requested,
    Approved,
    Rejected,
    Ended,
    Expired,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Ended => "ended",
            Self::Expired => "expired",
        }
    }
}

/// What awzy staff asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: SupportSessionId,
    pub requested_by: String,
    pub areas: Vec<String>,
    pub reason: String,
    pub duration_minutes: u32,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub request: Request,
    pub status: Status,
    pub approved_areas: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    pub ended_by: Option<String>,
    pub ended_at: Option<DateTime<Utc>>,
    pub action_count: u32,
}

impl Session {
    /// Whether the staff member may use it now, on `area`.
    pub fn allows(&self, area: &str) -> bool {
        self.status == Status::Approved && self.approved_areas.iter().any(|a| a == area)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SupportError {
    #[error("support session {0} not found")]
    NotFound(SupportSessionId),
    #[error("{0}")]
    Invalid(String),
    #[error("support session {id} is {status}")]
    WrongStatus { id: SupportSessionId, status: &'static str },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// A staff member's name or a portal area: what the corp portal sends, nothing that could hide
/// markup or a line break.
fn is_name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.@".contains(c))
}

impl Request {
    pub fn validate(&self) -> Result<(), SupportError> {
        let invalid = |m: String| Err(SupportError::Invalid(m));
        if !is_name(&self.requested_by) {
            return invalid(format!("requested_by {:?} isn't a staff member's name", self.requested_by));
        }
        if self.areas.is_empty() || !self.areas.iter().all(|a| is_name(a)) {
            return invalid("a support session asks for one or more portal areas".into());
        }
        if self.reason.trim().is_empty() || self.reason.chars().count() > 500 {
            return invalid("a support session needs a reason, at most 500 characters".into());
        }
        if self.duration_minutes == 0 || self.duration_minutes > MAX_MINUTES {
            return invalid(format!("a support session lasts 1 to {MAX_MINUTES} minutes"));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct SupportStore {
    pool: PgPool,
}

const COLUMNS: &str = "s.id, s.requested_by, s.areas, s.reason, s.duration_minutes, s.requested_at, s.status, \
     s.approved_areas, s.expires_at, s.decided_by, s.decided_at, s.ended_by, s.ended_at, \
     (SELECT count(*) FROM support_actions a WHERE a.session_id = s.id) AS action_count";

impl SupportStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Stores requests not seen before (the reporter brings each until the corp portal sees an
    /// answer), and returns the sessions the heartbeat reports: every one not over yet, and those
    /// that ended in the last day.
    pub async fn record_requests(&self, requests: &[Request]) -> Result<Vec<Session>, SupportError> {
        for r in requests {
            if let Err(e) = r.validate() {
                tracing::warn!(session = %r.id, error = %e, "ignored a support request");
                continue;
            }
            let inserted = sqlx::query(
                "INSERT INTO support_sessions (id, requested_by, areas, reason, duration_minutes, requested_at)
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (id) DO NOTHING",
            )
            .bind(r.id.uuid())
            .bind(&r.requested_by)
            .bind(&r.areas)
            .bind(&r.reason)
            .bind(r.duration_minutes as i32)
            .bind(r.requested_at)
            .execute(&self.pool)
            .await?
            .rows_affected();
            if inserted > 0 {
                tracing::info!(session = %r.id, requested_by = %r.requested_by, areas = ?r.areas, "support session requested");
            }
        }
        let rows = sqlx::query(&format!(
            "SELECT {COLUMNS} FROM support_sessions s
             WHERE s.status IN ('requested', 'approved') OR coalesce(s.ended_at, s.decided_at, s.expires_at) > now() - interval '1 day'
             ORDER BY s.requested_at DESC LIMIT 100"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(session_from_row).collect::<Result<_, _>>()?)
    }

    /// Newest first.
    pub async fn list(&self, limit: i64) -> Result<Vec<Session>, SupportError> {
        let rows =
            sqlx::query(&format!("SELECT {COLUMNS} FROM support_sessions s ORDER BY s.requested_at DESC LIMIT $1"))
                .bind(limit)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.iter().map(session_from_row).collect::<Result<_, _>>()?)
    }

    pub async fn get(&self, id: SupportSessionId) -> Result<Session, SupportError> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM support_sessions s WHERE s.id = $1"))
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(SupportError::NotFound(id))?;
        Ok(session_from_row(&row)?)
    }

    /// An admin's decision on a request. Approving names the areas, a subset of those asked for,
    /// and how long, at most what was asked for.
    pub async fn decide(
        &self,
        id: SupportSessionId,
        approve: bool,
        areas: &[String],
        minutes: u32,
        by: &str,
    ) -> Result<Session, SupportError> {
        let session = self.get(id).await?;
        if session.status != Status::Requested {
            return Err(SupportError::WrongStatus { id, status: session.status.as_str() });
        }
        if approve {
            if areas.is_empty() || !areas.iter().all(|a| session.request.areas.contains(a)) {
                return Err(SupportError::Invalid("approve one or more of the areas asked for".into()));
            }
            if minutes == 0 || minutes > session.request.duration_minutes {
                return Err(SupportError::Invalid(format!(
                    "approve 1 to {} minutes, as long as was asked for",
                    session.request.duration_minutes
                )));
            }
        }
        let expires_at = approve.then(|| Utc::now() + Duration::minutes(minutes.into()));
        sqlx::query(
            "UPDATE support_sessions SET status = $2, approved_areas = $3, expires_at = $4, decided_by = $5,
             decided_at = now() WHERE id = $1 AND status = 'requested'",
        )
        .bind(id.uuid())
        .bind(if approve { "approved" } else { "rejected" })
        .bind(if approve { areas.to_vec() } else { vec![] })
        .bind(expires_at)
        .bind(by)
        .execute(&self.pool)
        .await?;
        tracing::info!(session = %id, approve, areas = ?areas, minutes, by, "support session decided");
        self.get(id).await
    }

    /// An admin ends an approved session before it expires.
    pub async fn end(&self, id: SupportSessionId, by: &str) -> Result<Session, SupportError> {
        let session = self.get(id).await?;
        if session.status != Status::Approved {
            return Err(SupportError::WrongStatus { id, status: session.status.as_str() });
        }
        sqlx::query("UPDATE support_sessions SET status = 'ended', ended_by = $2, ended_at = now() WHERE id = $1")
            .bind(id.uuid())
            .bind(by)
            .execute(&self.pool)
            .await?;
        tracing::info!(session = %id, by, "support session ended");
        self.get(id).await
    }

    /// One operation the staff member made in an active session.
    pub async fn record_action(&self, id: SupportSessionId, operation: &str) -> Result<(), SupportError> {
        let operation: String = operation.chars().take(128).collect();
        sqlx::query("INSERT INTO support_actions (id, session_id, operation) VALUES ($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(id.uuid())
            .bind(&operation)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn session_from_row(row: &PgRow) -> Result<Session, sqlx::Error> {
    let stored: String = row.try_get("status")?;
    let expires_at: Option<DateTime<Utc>> = row.try_get("expires_at")?;
    let status = match stored.as_str() {
        "requested" => Status::Requested,
        "approved" if expires_at.is_some_and(|t| t <= Utc::now()) => Status::Expired,
        "approved" => Status::Approved,
        "rejected" => Status::Rejected,
        "ended" => Status::Ended,
        other => return Err(sqlx::Error::Decode(format!("unknown support session status {other:?}").into())),
    };
    let minutes: i32 = row.try_get("duration_minutes")?;
    let actions: i64 = row.try_get("action_count")?;
    Ok(Session {
        request: Request {
            id: SupportSessionId::from_uuid(row.try_get("id")?),
            requested_by: row.try_get("requested_by")?,
            areas: row.try_get("areas")?,
            reason: row.try_get("reason")?,
            duration_minutes: minutes.max(0) as u32,
            requested_at: row.try_get("requested_at")?,
        },
        status,
        approved_areas: row.try_get("approved_areas")?,
        expires_at,
        decided_by: row.try_get("decided_by")?,
        decided_at: row.try_get("decided_at")?,
        ended_by: row.try_get("ended_by")?,
        ended_at: row.try_get("ended_at")?,
        action_count: actions.max(0) as u32,
    })
}

// ---- The wire (awzy.challenge.v1)

use awzy_authz_proto::{
    challenge::v1::{SupportRequest as WireRequest, SupportSession as WireSession, SupportSessionStatus as WireStatus},
    prost_types::Timestamp,
};

fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }
}

pub(crate) fn request_from_wire(r: WireRequest) -> Result<Request, SupportError> {
    let id = r.qid.parse::<SupportSessionId>().map_err(|e| SupportError::Invalid(e.to_string()))?;
    let requested_at = r
        .requested_at
        .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
        .unwrap_or_else(Utc::now);
    Ok(Request {
        id,
        requested_by: r.requested_by,
        areas: r.areas,
        reason: r.reason,
        duration_minutes: r.duration_minutes,
        requested_at,
    })
}

pub(crate) fn session_to_wire(s: Session) -> WireSession {
    let actor = |stored: &Option<String>| awzy_authz_proto::actor_to_wire(stored.as_deref().unwrap_or("")).0;
    WireSession {
        request: Some(WireRequest {
            qid: s.request.id.to_string(),
            requested_by: s.request.requested_by.clone(),
            areas: s.request.areas.clone(),
            reason: s.request.reason.clone(),
            duration_minutes: s.request.duration_minutes,
            requested_at: Some(ts(s.request.requested_at)),
        }),
        status: match s.status {
            Status::Requested => WireStatus::Requested,
            Status::Approved => WireStatus::Approved,
            Status::Rejected => WireStatus::Rejected,
            Status::Ended => WireStatus::Ended,
            Status::Expired => WireStatus::Expired,
        }
        .into(),
        approved_areas: s.approved_areas.clone(),
        expires_at: s.expires_at.map(ts),
        decided_by_actor: actor(&s.decided_by),
        decided_at: s.decided_at.map(ts),
        ended_by_actor: actor(&s.ended_by),
        ended_at: s.ended_at.map(ts),
        action_count: s.action_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{tests::test_store, Lifetimes};

    fn request(minutes: u32) -> Request {
        Request {
            id: SupportSessionId::new(),
            requested_by: "max".into(),
            areas: vec!["decisions".into(), "policies".into()],
            reason: "PDP latency report".into(),
            duration_minutes: minutes,
            requested_at: Utc::now(),
        }
    }

    #[test]
    fn a_request_names_a_staff_member_areas_a_reason_and_a_bounded_duration() {
        assert!(request(60).validate().is_ok());
        assert!(request(0).validate().is_err());
        assert!(request(MAX_MINUTES + 1).validate().is_err());
        let mut r = request(60);
        r.requested_by = "max\nadmin".into();
        assert!(r.validate().is_err());
        let mut r = request(60);
        r.areas.clear();
        assert!(r.validate().is_err());
    }

    #[tokio::test]
    async fn an_admin_approves_at_most_what_was_asked_and_can_end_it() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let support = SupportStore::new(store.pool().clone());
        let asked = request(60);
        let sessions = support.record_requests(&[asked.clone(), asked.clone()]).await.unwrap();
        assert_eq!(sessions.len(), 1, "a request the reporter brings twice is stored once");
        assert_eq!(sessions[0].status, Status::Requested);

        let id = asked.id;
        assert!(support.decide(id, true, &["directory".into()], 30, "Human::alice").await.is_err(), "not asked for");
        assert!(support.decide(id, true, &["decisions".into()], 90, "Human::alice").await.is_err(), "too long");
        let approved = support.decide(id, true, &["decisions".into()], 30, "Human::alice").await.unwrap();
        assert!(approved.allows("decisions") && !approved.allows("policies"));
        assert!(matches!(support.decide(id, false, &[], 0, "Human::bob").await, Err(SupportError::WrongStatus { .. })));

        support.record_action(id, "DecisionsPageQuery").await.unwrap();
        let ended = support.end(id, "Human::alice").await.unwrap();
        assert_eq!((ended.status, ended.action_count), (Status::Ended, 1));
        assert!(!ended.allows("decisions"));
    }
}
