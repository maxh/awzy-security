//! Postgres storage for challenges and containments.

use std::time::Duration;

use awzy_model::{Actor, ChallengeId, ChallengeKind, Challenges, ContainmentId, DecisionId, EntityRef};
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};

/// Its migrations, tolerating ones a newer version already applied (ADR 0072): an older version
/// restarted mid-rollout, or rolled back to, still starts against the schema the newer one
/// expanded.
fn migrator() -> sqlx::migrate::Migrator {
    let mut migrator = sqlx::migrate!("./migrations");
    migrator.set_ignore_missing(true);
    migrator
}

/// The Postgres channel notified on every containment and lift, with the new feed version.
pub const CONTAINMENT_CHANNEL: &str = "awzy_containments";

/// The shortest justification accepted. A justification is recorded for review; a policy that
/// needs more than the person's word also asks for approval or verification.
pub const MIN_JUSTIFICATION_CHARS: usize = 20;

/// The request a challenge is for: what the PDP decided, without its context. A grant from a
/// passed challenge covers exactly this request and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestKey {
    pub subject: EntityRef,
    pub on_behalf_of: Option<EntityRef>,
    pub action: String,
    pub resource: EntityRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeStatus {
    Pending,
    Passed,
    Rejected,
    Expired,
}

impl ChallengeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    fn parse(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "pending" => Ok(Self::Pending),
            "passed" => Ok(Self::Passed),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            other => Err(sqlx::Error::Decode(format!("unknown challenge status {other:?}").into())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Challenge {
    pub id: ChallengeId,
    pub decision_id: DecisionId,
    pub request: RequestKey,
    pub kinds: Challenges,
    pub passed: Challenges,
    /// A pending challenge past `expires_at` reads as expired.
    pub status: ChallengeStatus,
    /// Who answers it. None when no one is known, so only an approval can pass it.
    pub answerer_id: Option<String>,
    pub justification: Option<String>,
    pub decided_by: Option<String>,
    pub decision_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub passed_at: Option<DateTime<Utc>>,
    pub grant_expires_at: Option<DateTime<Utc>>,
}

impl Challenge {
    /// Who a request's own approval can't come from: whoever made it, the person it was for, and
    /// whoever answers for it. A service account approving for its owner is the PDP's to refuse
    /// (ADR 0065), since only it knows owners.
    fn parties(&self) -> impl Iterator<Item = EntityRef> + '_ {
        let r = &self.request;
        let human = |id: &str| EntityRef::new(awzy_model::entity_type::HUMAN, id);
        std::iter::once(r.subject.clone()).chain(r.on_behalf_of.clone()).chain(self.answerer_id.as_deref().map(human))
    }

    fn ensure_answerable(&self, kind: ChallengeKind) -> Result<(), StoreError> {
        if self.status != ChallengeStatus::Pending {
            return Err(StoreError::NotPending { id: self.id, status: self.status.as_str() });
        }
        if !self.kinds.contains(kind) {
            return Err(StoreError::KindNotRequired { id: self.id, kind: kind.as_str() });
        }
        if self.passed.contains(kind) {
            return Err(StoreError::AlreadyPassed { id: self.id, kind: kind.as_str() });
        }
        Ok(())
    }
}

/// What a request has passed, and what it still has to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Required kinds with a live grant for this request.
    pub passed: Challenges,
    /// The pending challenge for the rest, when one was opened or already open.
    pub challenge_id: Option<ChallengeId>,
    /// An approver rejected this request and the rejection hasn't expired.
    pub rejected: bool,
    /// The challenge is new: this request opened it rather than finding it open (ADR 0073).
    pub opened: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Containment {
    pub id: ContainmentId,
    pub subject: EntityRef,
    pub reason: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub lifted_at: Option<DateTime<Utc>>,
    pub lifted_by: Option<String>,
    pub lift_reason: Option<String>,
    /// Recorded but not applied (ADR 0066): never in the feed.
    pub monitor: bool,
}

/// Every containment in force, as of a feed version.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActiveContainments {
    pub version: i64,
    pub active: Vec<Containment>,
}

/// How long challenges and grants last.
#[derive(Debug, Clone, Copy)]
pub struct Lifetimes {
    /// A pending challenge expires after this; a rejection blocks the request this long. Long
    /// enough for a service account's owner to answer later (the async flow for agents).
    pub pending: Duration,
    /// A passed challenge allows its request this long.
    pub grant: Duration,
}

impl Default for Lifetimes {
    fn default() -> Self {
        Self { pending: Duration::from_secs(24 * 3600), grant: Duration::from_secs(15 * 60) }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("challenge {0} not found")]
    ChallengeNotFound(ChallengeId),
    #[error("challenge {id} is {status}; only a pending challenge can be answered")]
    NotPending { id: ChallengeId, status: &'static str },
    #[error("challenge {id} doesn't ask for {kind}")]
    KindNotRequired { id: ChallengeId, kind: &'static str },
    #[error("challenge {id} already passed {kind}")]
    AlreadyPassed { id: ChallengeId, kind: &'static str },
    #[error("{0}")]
    NotAllowed(String),
    #[error("{0}")]
    Invalid(String),
    #[error("containment {0} not found")]
    ContainmentNotFound(ContainmentId),
    #[error("containment {0} was already lifted")]
    AlreadyLifted(ContainmentId),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Matches one request's rows; binds $1..$7 in [`bind_request`] order.
const REQUEST_MATCH: &str = "subject_type = $1 AND subject_id = $2 AND on_behalf_of_type = $3 \
    AND on_behalf_of_id = $4 AND action = $5 AND resource_type = $6 AND resource_id = $7";

fn bind_request<'q>(
    q: sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments>,
    r: &'q RequestKey,
) -> sqlx::query::Query<'q, Postgres, sqlx::postgres::PgArguments> {
    let (obo_type, obo_id) = r.on_behalf_of.as_ref().map_or(("", ""), |h| (h.kind.as_str(), h.id.as_str()));
    q.bind(&r.subject.kind)
        .bind(&r.subject.id)
        .bind(obo_type)
        .bind(obo_id)
        .bind(&r.action)
        .bind(&r.resource.kind)
        .bind(&r.resource.id)
}

pub struct ChallengeStore {
    pool: PgPool,
    lifetimes: Lifetimes,
    /// Whether to record notifications (ADR 0073): only when a channel is configured.
    pub(crate) notify: bool,
}

impl ChallengeStore {
    pub fn new(pool: PgPool, lifetimes: Lifetimes) -> Self {
        Self { pool, lifetimes, notify: false }
    }

    /// Records what to tell people as challenges and containments happen (ADR 0073).
    pub fn with_notifications(mut self) -> Self {
        self.notify = true;
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        migrator().run(&self.pool).await
    }

    /// What `request` has passed of `required`, and, with `open`, the pending challenge for the
    /// rest: an open one is reused, so retries don't pile up challenges. Nothing is opened while
    /// a rejection stands.
    pub async fn resolve(
        &self,
        decision_id: DecisionId,
        request: &RequestKey,
        required: Challenges,
        answerer_id: Option<&str>,
        open: bool,
    ) -> Result<Resolution, StoreError> {
        let mut tx = self.pool.begin().await?;
        let sql = format!(
            "SELECT status, passed FROM challenges WHERE {REQUEST_MATCH} AND \
             ((status = 'passed' AND grant_expires_at > now()) OR (status = 'rejected' AND expires_at > now()))"
        );
        let rows = bind_request(sqlx::query(&sql), request).fetch_all(&mut *tx).await?;
        let mut passed = Challenges::default();
        let mut rejected = false;
        for row in &rows {
            match row.try_get::<&str, _>("status")? {
                "passed" => passed = passed.union(kinds(row.try_get("passed")?)?),
                _ => rejected = true,
            }
        }
        let passed = passed.intersection(required);
        let remaining = required.without(passed);
        let mut opened = false;
        let challenge_id = if open && !rejected && !remaining.is_empty() {
            let expire = format!(
                "UPDATE challenges SET status = 'expired' WHERE {REQUEST_MATCH} \
                 AND status = 'pending' AND expires_at <= now()"
            );
            bind_request(sqlx::query(&expire), request).execute(&mut *tx).await?;
            let insert = "INSERT INTO challenges (subject_type, subject_id, on_behalf_of_type, on_behalf_of_id, \
                    action, resource_type, resource_id, id, decision_id, kinds, status, answerer_id, expires_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending', $11, now() + make_interval(secs => $12)) \
                 ON CONFLICT (subject_type, subject_id, on_behalf_of_type, on_behalf_of_id, action, resource_type, \
                    resource_id) WHERE status = 'pending' \
                 DO UPDATE SET kinds = ARRAY(SELECT DISTINCT k FROM unnest(challenges.kinds || EXCLUDED.kinds) AS k \
                    ORDER BY k) \
                 RETURNING id, (xmax = 0) AS inserted";
            let row = bind_request(sqlx::query(insert), request)
                .bind(ChallengeId::new().uuid())
                .bind(decision_id.uuid())
                .bind(kind_names(remaining))
                .bind(answerer_id.unwrap_or_default())
                .bind(self.lifetimes.pending.as_secs_f64())
                .fetch_one(&mut *tx)
                .await?;
            // An inserted row has no xmax; one the conflict updated does.
            opened = row.try_get("inserted")?;
            Some(ChallengeId::from_uuid(row.try_get("id")?))
        } else {
            None
        };
        tx.commit().await?;
        Ok(Resolution { passed, challenge_id, rejected, opened })
    }

    pub async fn get(&self, id: ChallengeId) -> Result<Challenge, StoreError> {
        let row = sqlx::query("SELECT * FROM challenges WHERE id = $1")
            .bind(id.uuid())
            .fetch_optional(&self.pool)
            .await?
            .ok_or(StoreError::ChallengeNotFound(id))?;
        Ok(challenge_from_row(&row)?)
    }

    /// Newest first, optionally only those sent to `answerer_id` or in `status`.
    pub async fn list(
        &self,
        answerer_id: Option<&str>,
        status: Option<ChallengeStatus>,
        limit: i64,
    ) -> Result<Vec<Challenge>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM challenges WHERE ($1::text IS NULL OR answerer_id = $1) AND ($2::text IS NULL \
                OR ($2 = 'pending' AND status = 'pending' AND expires_at > now()) \
                OR ($2 = 'expired' AND (status = 'expired' OR (status = 'pending' AND expires_at <= now()))) \
                OR ($2 IN ('passed', 'rejected') AND status = $2)) \
             ORDER BY created_at DESC LIMIT $3",
        )
        .bind(answerer_id)
        .bind(status.map(ChallengeStatus::as_str))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(challenge_from_row).collect::<Result<_, _>>()?)
    }

    /// Records the answerer's justification. Passes the challenge if nothing else is left.
    pub async fn justify(&self, id: ChallengeId, answered_by: &str, text: &str) -> Result<Challenge, StoreError> {
        let text = text.trim();
        if text.chars().count() < MIN_JUSTIFICATION_CHARS {
            return Err(StoreError::Invalid(format!(
                "a justification needs at least {MIN_JUSTIFICATION_CHARS} characters saying why this access is needed"
            )));
        }
        let mut tx = self.pool.begin().await?;
        let c = lock_for_answerer(&mut tx, id, answered_by, ChallengeKind::Justification).await?;
        sqlx::query("UPDATE challenges SET justification = $2 WHERE id = $1")
            .bind(id.uuid())
            .bind(text)
            .execute(&mut *tx)
            .await?;
        let c = self.mark_passed(&mut tx, &c, ChallengeKind::Justification).await?;
        tx.commit().await?;
        Ok(c)
    }

    /// Opens a transaction for a caller that checks something of its own before passing a kind
    /// (a security key's assertion).
    pub(crate) async fn begin(&self) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Locks challenge `id` for `person` to answer `kind`, in the caller's transaction.
    pub(crate) async fn lock_for(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: ChallengeId,
        person: &str,
        kind: ChallengeKind,
    ) -> Result<Challenge, StoreError> {
        lock_for_answerer(tx, id, person, kind).await
    }

    /// Records `kind` as passed on locked challenge `c`, and passes the challenge if nothing is
    /// left: the request is then granted for the grant lifetime.
    pub(crate) async fn mark_passed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        c: &Challenge,
        kind: ChallengeKind,
    ) -> Result<Challenge, StoreError> {
        let passed = c.passed.with(kind);
        let row = sqlx::query(
            "UPDATE challenges SET passed = $2, \
                status = CASE WHEN $3 THEN 'passed' ELSE status END, \
                passed_at = CASE WHEN $3 THEN now() END, \
                grant_expires_at = CASE WHEN $3 THEN now() + make_interval(secs => $4) END \
             WHERE id = $1 RETURNING *",
        )
        .bind(c.id.uuid())
        .bind(kind_names(passed))
        .bind(c.kinds.without(passed).is_empty())
        .bind(self.lifetimes.grant.as_secs_f64())
        .fetch_one(&mut **tx)
        .await?;
        Ok(challenge_from_row(&row)?)
    }

    /// Records an approver's decision. An approval passes the challenge if nothing else is
    /// left; a rejection ends it and blocks the request for the pending lifetime. The caller has
    /// already checked with the PDP that `decided_by` may approve this resource.
    pub async fn decide(
        &self,
        id: ChallengeId,
        decided_by: &str,
        approve: bool,
        note: &str,
    ) -> Result<Challenge, StoreError> {
        let mut tx = self.pool.begin().await?;
        let c = lock(&mut tx, id).await?;
        c.ensure_answerable(ChallengeKind::Approval)?;
        let approver = Actor::parse(decided_by).subject;
        if c.parties().any(|p| p == approver) {
            return Err(StoreError::NotAllowed(format!(
                "{decided_by} made or answers for the request in challenge {id}, so can't approve it"
            )));
        }
        let note = Some(note.trim()).filter(|n| !n.is_empty());
        let c = if approve {
            sqlx::query("UPDATE challenges SET decided_by = $2, decision_note = $3 WHERE id = $1")
                .bind(id.uuid())
                .bind(decided_by)
                .bind(note)
                .execute(&mut *tx)
                .await?;
            self.mark_passed(&mut tx, &c, ChallengeKind::Approval).await?
        } else {
            let row = sqlx::query(
                "UPDATE challenges SET decided_by = $2, decision_note = $3, status = 'rejected', \
                    expires_at = now() + make_interval(secs => $4) \
                 WHERE id = $1 RETURNING *",
            )
            .bind(id.uuid())
            .bind(decided_by)
            .bind(note)
            .bind(self.lifetimes.pending.as_secs_f64())
            .fetch_one(&mut *tx)
            .await?;
            challenge_from_row(&row)?
        };
        tx.commit().await?;
        Ok(c)
    }

    /// Contains `subject`, or with `monitor` records that it would have (ADR 0066). Returns the
    /// containment and whether it is new: an accessor already contained in that mode keeps their
    /// active containment.
    pub async fn contain(
        &self,
        subject: &EntityRef,
        reason: &str,
        created_by: &str,
        monitor: bool,
    ) -> Result<(Containment, bool), StoreError> {
        if subject.kind.is_empty() || subject.id.is_empty() {
            return Err(StoreError::Invalid("subject type and id are required".into()));
        }
        if reason.trim().is_empty() || created_by.is_empty() {
            return Err(StoreError::Invalid("reason and created_by are required".into()));
        }
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO containments (id, subject_type, subject_id, reason, created_by, monitor) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (subject_type, subject_id, monitor) WHERE lifted_at IS NULL DO NOTHING RETURNING *",
        )
        .bind(ContainmentId::new().uuid())
        .bind(&subject.kind)
        .bind(&subject.id)
        .bind(reason.trim())
        .bind(created_by)
        .bind(monitor)
        .fetch_optional(&mut *tx)
        .await?;
        let result = match inserted {
            Some(row) => {
                // PDPs never see a monitored containment, so the feed needn't move.
                if !monitor {
                    bump_feed(&mut tx).await?;
                }
                (containment_from_row(&row)?, true)
            }
            None => {
                let row = sqlx::query(
                    "SELECT * FROM containments \
                     WHERE subject_type = $1 AND subject_id = $2 AND monitor = $3 AND lifted_at IS NULL",
                )
                .bind(&subject.kind)
                .bind(&subject.id)
                .bind(monitor)
                .fetch_one(&mut *tx)
                .await?;
                (containment_from_row(&row)?, false)
            }
        };
        tx.commit().await?;
        Ok(result)
    }

    pub async fn lift(&self, id: ContainmentId, lifted_by: &str, reason: &str) -> Result<Containment, StoreError> {
        if reason.trim().is_empty() || lifted_by.is_empty() {
            return Err(StoreError::Invalid("reason and lifted_by are required".into()));
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "UPDATE containments SET lifted_at = now(), lifted_by = $2, lift_reason = $3 \
             WHERE id = $1 AND lifted_at IS NULL RETURNING *",
        )
        .bind(id.uuid())
        .bind(lifted_by)
        .bind(reason.trim())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            let exists = sqlx::query("SELECT 1 FROM containments WHERE id = $1")
                .bind(id.uuid())
                .fetch_optional(&mut *tx)
                .await?
                .is_some();
            return Err(if exists { StoreError::AlreadyLifted(id) } else { StoreError::ContainmentNotFound(id) });
        };
        bump_feed(&mut tx).await?;
        tx.commit().await?;
        Ok(containment_from_row(&row)?)
    }

    /// Newest first. Monitored containments only with `include_monitor`.
    pub async fn containments(
        &self,
        active_only: bool,
        subject: Option<&EntityRef>,
        limit: i64,
        include_monitor: bool,
    ) -> Result<Vec<Containment>, StoreError> {
        let rows = sqlx::query(
            "SELECT * FROM containments WHERE (NOT $1 OR lifted_at IS NULL) \
                AND ($2::text IS NULL OR (subject_type = $2 AND subject_id = $3)) \
                AND ($5 OR NOT monitor) \
             ORDER BY created_at DESC LIMIT $4",
        )
        .bind(active_only)
        .bind(subject.map(|s| s.kind.as_str()))
        .bind(subject.map(|s| s.id.as_str()))
        .bind(limit)
        .bind(include_monitor)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(containment_from_row).collect::<Result<_, _>>()?)
    }

    /// The feed version and every active containment, read from one snapshot.
    pub async fn active_containments(&self) -> Result<ActiveContainments, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY").execute(&mut *tx).await?;
        let version: i64 = sqlx::query("SELECT version FROM containment_feed").fetch_one(&mut *tx).await?.try_get(0)?;
        let rows =
            sqlx::query("SELECT * FROM containments WHERE lifted_at IS NULL AND NOT monitor ORDER BY created_at")
                .fetch_all(&mut *tx)
                .await?;
        tx.commit().await?;
        Ok(ActiveContainments { version, active: rows.iter().map(containment_from_row).collect::<Result<_, _>>()? })
    }
}

async fn lock(tx: &mut Transaction<'_, Postgres>, id: ChallengeId) -> Result<Challenge, StoreError> {
    let row = sqlx::query("SELECT * FROM challenges WHERE id = $1 FOR UPDATE")
        .bind(id.uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::ChallengeNotFound(id))?;
    Ok(challenge_from_row(&row)?)
}

/// Locks challenge `id` and checks `person` may answer `kind` on it now.
async fn lock_for_answerer(
    tx: &mut Transaction<'_, Postgres>,
    id: ChallengeId,
    person: &str,
    kind: ChallengeKind,
) -> Result<Challenge, StoreError> {
    let c = lock(tx, id).await?;
    c.ensure_answerable(kind)?;
    if c.answerer_id.as_deref() != Some(person) {
        return Err(StoreError::NotAllowed(match &c.answerer_id {
            Some(answerer) => format!("only {answerer} can answer challenge {id} with {}", kind.as_str()),
            None => format!("challenge {id} has no one to answer it; it can only be approved"),
        }));
    }
    Ok(c)
}

/// Bumps the feed version and tells every replica's feed, in the caller's transaction, so the
/// notification goes out only if the change commits.
async fn bump_feed(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    let version: i64 = sqlx::query("UPDATE containment_feed SET version = version + 1 RETURNING version")
        .fetch_one(&mut **tx)
        .await?
        .try_get(0)?;
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(CONTAINMENT_CHANNEL)
        .bind(version.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn kind_names(kinds: Challenges) -> Vec<&'static str> {
    kinds.iter().map(ChallengeKind::as_str).collect()
}

fn kinds(names: Vec<String>) -> Result<Challenges, sqlx::Error> {
    names
        .iter()
        .map(|n| {
            ChallengeKind::parse(n).ok_or_else(|| sqlx::Error::Decode(format!("unknown challenge kind {n:?}").into()))
        })
        .collect()
}

fn non_empty(s: String) -> Option<String> {
    Some(s).filter(|s| !s.is_empty())
}

fn challenge_from_row(row: &PgRow) -> Result<Challenge, sqlx::Error> {
    let expires_at: DateTime<Utc> = row.try_get("expires_at")?;
    let status = match ChallengeStatus::parse(row.try_get("status")?)? {
        ChallengeStatus::Pending if expires_at <= Utc::now() => ChallengeStatus::Expired,
        s => s,
    };
    let on_behalf_of =
        match (row.try_get::<String, _>("on_behalf_of_type")?, row.try_get::<String, _>("on_behalf_of_id")?) {
            (kind, id) if !kind.is_empty() => Some(EntityRef::new(kind, id)),
            _ => None,
        };
    Ok(Challenge {
        id: ChallengeId::from_uuid(row.try_get("id")?),
        decision_id: DecisionId::from_uuid(row.try_get("decision_id")?),
        request: RequestKey {
            subject: EntityRef::new(row.try_get::<String, _>("subject_type")?, row.try_get::<String, _>("subject_id")?),
            on_behalf_of,
            action: row.try_get("action")?,
            resource: EntityRef::new(
                row.try_get::<String, _>("resource_type")?,
                row.try_get::<String, _>("resource_id")?,
            ),
        },
        kinds: kinds(row.try_get("kinds")?)?,
        passed: kinds(row.try_get("passed")?)?,
        status,
        answerer_id: non_empty(row.try_get("answerer_id")?),
        justification: row.try_get("justification")?,
        decided_by: row.try_get("decided_by")?,
        decision_note: row.try_get("decision_note")?,
        created_at: row.try_get("created_at")?,
        expires_at,
        passed_at: row.try_get("passed_at")?,
        grant_expires_at: row.try_get("grant_expires_at")?,
    })
}

fn containment_from_row(row: &PgRow) -> Result<Containment, sqlx::Error> {
    Ok(Containment {
        id: ContainmentId::from_uuid(row.try_get("id")?),
        subject: EntityRef::new(row.try_get::<String, _>("subject_type")?, row.try_get::<String, _>("subject_id")?),
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        lifted_at: row.try_get("lifted_at")?,
        lifted_by: row.try_get("lifted_by")?,
        lift_reason: row.try_get("lift_reason")?,
        monitor: row.try_get("monitor")?,
    })
}

/// Integration tests against Postgres. Run with the local stack up:
///   bazel test --config=local-stack //authz/services/challenge:challenge_test
/// Without AWZY_TEST_POSTGRES_URL they pass without checking anything, so CI stays green.
#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use uuid::Uuid;

    use super::*;

    const WHY: &str = "Reviewing the Q3 board deck for the audit committee";

    /// A store in a fresh Postgres schema, so tests never see each other's rows.
    pub(crate) async fn test_store(lifetimes: Lifetimes) -> Option<ChallengeStore> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", Uuid::now_v7().simple());
        let admin = PgPool::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        let store = ChallengeStore::new(pool, lifetimes);
        store.migrate().await.unwrap();
        Some(store)
    }

    pub(crate) fn request(subject: EntityRef, on_behalf_of: Option<EntityRef>) -> RequestKey {
        RequestKey { subject, on_behalf_of, action: "read".into(), resource: EntityRef::new("Resource", "board-deck") }
    }

    fn alice() -> EntityRef {
        EntityRef::new("Human", "alice")
    }

    fn of(kinds: &[ChallengeKind]) -> Challenges {
        kinds.iter().copied().collect()
    }

    #[tokio::test]
    async fn retries_share_one_pending_challenge() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(alice(), None);
        let required = of(&[ChallengeKind::Justification]);
        let first = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        let again = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert!(first.challenge_id.is_some());
        assert_eq!(first.challenge_id, again.challenge_id);
        assert!(first.passed.is_empty() && !first.rejected);

        let c = store.get(first.challenge_id.unwrap()).await.unwrap();
        assert_eq!((c.status, c.kinds, c.answerer_id.as_deref()), (ChallengeStatus::Pending, required, Some("alice")));
    }

    #[tokio::test]
    async fn monitor_lookups_open_nothing() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(alice(), None);
        let r = store.resolve(DecisionId::new(), &req, of(&[ChallengeKind::Approval]), None, false).await.unwrap();
        assert_eq!(r, Resolution { passed: Challenges::default(), challenge_id: None, rejected: false, opened: false });
        assert!(store.list(None, None, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_justification_grants_exactly_the_challenged_request() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(alice(), None);
        let required = of(&[ChallengeKind::Justification]);
        let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap().challenge_id;
        let id = id.unwrap();

        assert!(matches!(store.justify(id, "alice", "because").await, Err(StoreError::Invalid(_))));
        assert!(matches!(store.justify(id, "mallory", WHY).await, Err(StoreError::NotAllowed(_))));
        let c = store.justify(id, "alice", WHY).await.unwrap();
        assert_eq!((c.status, c.justification.as_deref()), (ChallengeStatus::Passed, Some(WHY)));
        assert!(c.grant_expires_at.unwrap() > Utc::now());
        assert!(matches!(store.justify(id, "alice", WHY).await, Err(StoreError::NotPending { .. })));

        let retry = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert_eq!((retry.passed, retry.challenge_id), (required, None), "passed on retry, nothing new opened");

        let other = RequestKey { resource: EntityRef::new("Resource", "payroll"), ..req.clone() };
        let r = store.resolve(DecisionId::new(), &other, required, Some("alice"), true).await.unwrap();
        assert!(r.passed.is_empty() && r.challenge_id.is_some(), "another resource needs its own challenge");
        let write = RequestKey { action: "write".into(), ..req };
        let r = store.resolve(DecisionId::new(), &write, required, Some("alice"), true).await.unwrap();
        assert!(r.passed.is_empty(), "another action needs its own challenge");
    }

    #[tokio::test]
    async fn every_kind_must_pass_before_the_grant() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(alice(), None);
        let required = of(&[ChallengeKind::Justification, ChallengeKind::Approval]);
        let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        let id = id.challenge_id.unwrap();

        let c = store.justify(id, "alice", WHY).await.unwrap();
        assert_eq!((c.status, c.passed), (ChallengeStatus::Pending, of(&[ChallengeKind::Justification])));
        let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert!(r.passed.is_empty(), "half a challenge grants nothing");
        assert_eq!(r.challenge_id, Some(id));

        let c = store.decide(id, "carol", true, "ok for the audit").await.unwrap();
        assert_eq!((c.status, c.decided_by.as_deref()), (ChallengeStatus::Passed, Some("carol")));
        let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert_eq!((r.passed, r.challenge_id), (required, None));
    }

    #[tokio::test]
    async fn no_one_approves_their_own_request() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let agent = EntityRef::new("ServiceAccount", "sales-genie");
        let req = request(agent, Some(EntityRef::new("Human", "bob")));
        let required = of(&[ChallengeKind::Approval]);
        let id = store.resolve(DecisionId::new(), &req, required, Some("bob"), true).await.unwrap();
        let id = id.challenge_id.unwrap();
        let err = store.decide(id, "bob", true, "").await;
        assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the person the agent acts for");
        let err = store.justify(id, "bob", WHY).await;
        assert!(
            matches!(err, Err(StoreError::KindNotRequired { .. })),
            "approval can't be swapped for a justification"
        );

        let human = request(alice(), None);
        let id = store.resolve(DecisionId::new(), &human, required, Some("alice"), true).await.unwrap();
        let err = store.decide(id.challenge_id.unwrap(), "alice", true, "").await;
        assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the person asking");
    }

    #[tokio::test]
    async fn an_agent_approves_but_never_its_own_request() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(EntityRef::new("ServiceAccount", "sales-genie"), Some(EntityRef::new("Human", "bob")));
        let required = of(&[ChallengeKind::Approval]);
        let id = store.resolve(DecisionId::new(), &req, required, Some("bob"), true).await.unwrap();
        let id = id.challenge_id.unwrap();
        let err = store.decide(id, "ServiceAccount::sales-genie", true, "").await;
        assert!(matches!(err, Err(StoreError::NotAllowed(_))), "the agent asking");

        let c = store.decide(id, "ServiceAccount::overseer", true, "routine").await.unwrap();
        assert_eq!((c.status, c.decided_by.as_deref()), (ChallengeStatus::Passed, Some("ServiceAccount::overseer")));
    }

    #[tokio::test]
    async fn a_rejection_blocks_the_request_until_it_expires() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let req = request(alice(), None);
        let required = of(&[ChallengeKind::Approval]);
        let id = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        let c = store.decide(id.challenge_id.unwrap(), "carol", false, "not for this quarter").await.unwrap();
        assert_eq!(c.status, ChallengeStatus::Rejected);

        let r = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert!(r.rejected && r.challenge_id.is_none(), "no new challenge while the rejection stands");
    }

    #[tokio::test]
    async fn expired_challenges_and_grants_count_for_nothing() {
        let lifetimes = Lifetimes { pending: Duration::ZERO, grant: Duration::ZERO };
        let Some(store) = test_store(lifetimes).await else { return };
        let req = request(alice(), None);
        let required = of(&[ChallengeKind::Justification]);
        let first = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        let first = first.challenge_id.unwrap();
        assert_eq!(store.get(first).await.unwrap().status, ChallengeStatus::Expired);
        assert!(matches!(store.justify(first, "alice", WHY).await, Err(StoreError::NotPending { .. })));

        let second = store.resolve(DecisionId::new(), &req, required, Some("alice"), true).await.unwrap();
        assert_ne!(second.challenge_id, Some(first), "an expired challenge is replaced");
        assert_eq!(store.list(Some("alice"), Some(ChallengeStatus::Expired), 10).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn containments_are_idempotent_lifted_once_and_versioned() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        assert_eq!(store.active_containments().await.unwrap(), ActiveContainments::default());

        let agent = EntityRef::new("ServiceAccount", "sales-genie");
        let (c, created) = store.contain(&agent, "bulk export of pipeline", "carol", false).await.unwrap();
        assert!(created);
        let (again, created) = store.contain(&agent, "again", "dave", false).await.unwrap();
        assert_eq!((again.id, created), (c.id, false));
        let active = store.active_containments().await.unwrap();
        assert_eq!((active.version, active.active.len()), (1, 1));

        assert!(matches!(store.lift(c.id, "carol", " ").await, Err(StoreError::Invalid(_))));
        let lifted = store.lift(c.id, "carol", "reviewed: expected export").await.unwrap();
        assert_eq!(lifted.lifted_by.as_deref(), Some("carol"));
        assert!(matches!(store.lift(c.id, "carol", "again").await, Err(StoreError::AlreadyLifted(_))));
        let active = store.active_containments().await.unwrap();
        assert_eq!((active.version, active.active.len()), (2, 0));

        let (next, created) = store.contain(&agent, "again, later", "carol", false).await.unwrap();
        assert!(created && next.id != c.id, "a lifted containment doesn't block a new one");
        assert_eq!(store.containments(false, Some(&agent), 10, false).await.unwrap().len(), 2);
        assert_eq!(store.containments(true, None, 10, false).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_monitored_containment_stays_out_of_the_feed_and_beside_a_real_one() {
        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let nora = EntityRef::new("Human", "nora");
        let (would, created) =
            store.contain(&nora, "rejected challenge and an export", "AwzyService::awzy-reasoner", true).await.unwrap();
        assert!(created && would.monitor);
        let (again, created) = store.contain(&nora, "same evidence", "AwzyService::awzy-reasoner", true).await.unwrap();
        assert_eq!((again.id, created), (would.id, false));
        let active = store.active_containments().await.unwrap();
        assert_eq!((active.version, active.active.len()), (0, 0), "PDPs never hear of it");

        // The security team agrees and contains her for real; the monitored record stays.
        let (real, created) = store.contain(&nora, "confirmed on review", "Human::carol", false).await.unwrap();
        assert!(created && !real.monitor && real.id != would.id);
        assert_eq!(store.active_containments().await.unwrap().active, vec![real]);
        assert_eq!(store.containments(true, Some(&nora), 10, true).await.unwrap().len(), 2);
        assert_eq!(store.containments(true, Some(&nora), 10, false).await.unwrap().len(), 1);
    }
}
