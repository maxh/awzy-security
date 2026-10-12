//! Agents waiting for a challenge's answer (ADR 0111). An agent with no person in its session
//! gets a challenge, waits here while awzy tells the person, and retries once it's passed. The
//! grant stays in awzy-challenge, so the agent never holds anything it could replay, and only the
//! principal the challenge was opened for, with the same person, learns anything about it.

use std::time::{Duration, Instant};

use awzy_authz_proto::{
    authz::v1::{ChallengeOutcome, EntityRef as WireRef, WaitForChallengeResponse},
    challenge::v1::{Challenge, ChallengeStatus},
    challenges_from_wire, challenges_to_wire,
};
use awzy_model::EntityRef;

/// The longest one wait lasts, under proxies' idle timeouts; the caller asks again.
pub const MAX_WAIT: Duration = Duration::from_secs(25);
/// How often awzy-challenge is asked while waiting.
const POLL: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum WaitError {
    /// No such challenge, or not this principal's: the two look the same.
    #[error("no such challenge for this principal")]
    NotFound,
    #[error("awzy-challenge isn't configured, so no challenge can be waited for")]
    NotConfigured,
    #[error("awzy-challenge is unavailable: {0}")]
    Unavailable(String),
}

fn same(wire: &Option<WireRef>, model: Option<&EntityRef>) -> bool {
    match (wire, model) {
        (None, None) => true,
        (Some(w), Some(m)) => w.r#type == m.kind && w.id == m.id,
        _ => false,
    }
}

/// What became of `c`, for the agent waiting on it.
fn outcome(c: &Challenge) -> WaitForChallengeResponse {
    let status = ChallengeStatus::try_from(c.status).unwrap_or(ChallengeStatus::Unspecified);
    let mut res = WaitForChallengeResponse::default();
    match status {
        ChallengeStatus::Passed => {
            res.set_outcome(ChallengeOutcome::Passed);
            res.grant_expires_at = c.grant_expires_at;
        }
        ChallengeStatus::Rejected => {
            res.set_outcome(ChallengeOutcome::Rejected);
            res.expires_at = c.expires_at;
        }
        ChallengeStatus::Expired => res.set_outcome(ChallengeOutcome::Expired),
        ChallengeStatus::Pending | ChallengeStatus::Unspecified => {
            res.set_outcome(ChallengeOutcome::Pending);
            res.expires_at = c.expires_at;
            res.pending = challenges_to_wire(challenges_from_wire(&c.kinds).without(challenges_from_wire(&c.passed)));
        }
    }
    res
}

/// Asks `get` for the challenge until it's no longer pending or `wait` (at most [`MAX_WAIT`]) is
/// up. A challenge opened for another principal, or for the same one acting for someone else, is
/// [`WaitError::NotFound`].
pub async fn wait<F, Fut>(
    principal: &EntityRef,
    on_behalf_of: Option<&EntityRef>,
    wait: Duration,
    get: F,
) -> Result<WaitForChallengeResponse, WaitError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Option<Challenge>, WaitError>>,
{
    wait_polling(principal, on_behalf_of, wait, POLL, get).await
}

async fn wait_polling<F, Fut>(
    principal: &EntityRef,
    on_behalf_of: Option<&EntityRef>,
    wait: Duration,
    poll: Duration,
    mut get: F,
) -> Result<WaitForChallengeResponse, WaitError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Option<Challenge>, WaitError>>,
{
    let deadline = Instant::now() + wait.min(MAX_WAIT);
    loop {
        let c = get().await?.ok_or(WaitError::NotFound)?;
        if !same(&c.principal, Some(principal)) || !same(&c.on_behalf_of, on_behalf_of) {
            return Err(WaitError::NotFound);
        }
        let res = outcome(&c);
        if res.outcome() != ChallengeOutcome::Pending || Instant::now() + poll > deadline {
            return Ok(res);
        }
        tokio::time::sleep(poll).await;
    }
}

/// `POST /v1/access/challenge`: the same wait over HTTP, for enforcement points that use the
/// JSON API.
#[derive(serde::Deserialize)]
pub(crate) struct WaitBody {
    challenge_qid: String,
    principal: EntityRef,
    #[serde(default)]
    on_behalf_of: Option<EntityRef>,
    #[serde(default)]
    wait_secs: u64,
}

pub(crate) async fn http(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Json(body): axum::Json<WaitBody>,
) -> Result<axum::Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;
    let id: awzy_model::ChallengeId =
        body.challenge_qid.parse().map_err(|e| (StatusCode::BAD_REQUEST, format!("challenge_qid: {e}")))?;
    let challenges = state.challenges.clone();
    let res =
        wait(&body.principal, body.on_behalf_of.as_ref(), Duration::from_secs(body.wait_secs), || challenges.get(id))
            .await
            .map_err(|e| match e {
                WaitError::NotFound => (StatusCode::NOT_FOUND, e.to_string()),
                WaitError::NotConfigured | WaitError::Unavailable(_) => {
                    (StatusCode::SERVICE_UNAVAILABLE, e.to_string())
                }
            })?;
    let time = |t: Option<awzy_authz_proto::prost_types::Timestamp>| {
        t.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
    };
    let outcome = res.outcome().as_str_name().trim_start_matches("CHALLENGE_OUTCOME_").to_lowercase();
    let pending: Vec<_> = challenges_from_wire(&res.pending).iter().collect();
    Ok(axum::Json(serde_json::json!({
        "outcome": outcome,
        "expires_at": time(res.expires_at),
        "grant_expires_at": time(res.grant_expires_at),
        "pending": pending,
    })))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use awzy_authz_proto::authz::v1::ChallengeKind;

    use super::*;

    fn challenge(status: ChallengeStatus) -> Challenge {
        Challenge {
            qid: "challenge_0192a000-0000-7000-8000-000000000001".into(),
            principal: Some(WireRef { r#type: "ServiceAccount".into(), id: "renewals-bot".into() }),
            on_behalf_of: Some(WireRef { r#type: "Human".into(), id: "dana".into() }),
            kinds: vec![ChallengeKind::Approval as i32, ChallengeKind::Justification as i32],
            passed: vec![ChallengeKind::Justification as i32],
            status: status as i32,
            ..Default::default()
        }
    }

    fn bot() -> EntityRef {
        EntityRef::new("ServiceAccount", "renewals-bot")
    }

    fn dana() -> EntityRef {
        EntityRef::new("Human", "dana")
    }

    const FAST: Duration = Duration::from_millis(5);

    #[tokio::test]
    async fn an_agent_waits_until_the_person_approves_then_retries() {
        let asked = Arc::new(AtomicUsize::new(0));
        let get = || {
            let n = asked.fetch_add(1, Ordering::SeqCst);
            async move { Ok(Some(challenge(if n < 3 { ChallengeStatus::Pending } else { ChallengeStatus::Passed }))) }
        };
        let res = wait_polling(&bot(), Some(&dana()), Duration::from_secs(25), FAST, get).await.unwrap();
        assert_eq!(res.outcome(), ChallengeOutcome::Passed);
        assert_eq!(asked.load(Ordering::SeqCst), 4, "asked again until it was answered");
    }

    #[tokio::test]
    async fn a_wait_ends_pending_with_what_is_left_to_pass() {
        let get = || async { Ok(Some(challenge(ChallengeStatus::Pending))) };
        let res = wait_polling(&bot(), Some(&dana()), Duration::from_millis(30), FAST, get).await.unwrap();
        assert_eq!(res.outcome(), ChallengeOutcome::Pending);
        assert_eq!(res.pending, vec![ChallengeKind::Approval as i32], "the justification was passed");
        let looked = wait(&bot(), Some(&dana()), Duration::ZERO, get).await.unwrap();
        assert_eq!(looked.outcome(), ChallengeOutcome::Pending, "0 looks without waiting");
    }

    #[tokio::test]
    async fn only_the_principal_it_was_opened_for_sees_it() {
        let get = || async { Ok(Some(challenge(ChallengeStatus::Passed))) };
        let other = EntityRef::new("ServiceAccount", "other-bot");
        assert!(matches!(wait(&other, Some(&dana()), Duration::ZERO, get).await, Err(WaitError::NotFound)));
        assert!(
            matches!(wait(&bot(), None, Duration::ZERO, get).await, Err(WaitError::NotFound)),
            "the same agent on its own isn't the agent acting for dana"
        );
        assert!(matches!(wait(&dana(), None, Duration::ZERO, get).await, Err(WaitError::NotFound)));
        let missing = || async { Ok(None) };
        assert!(matches!(wait(&bot(), Some(&dana()), Duration::ZERO, missing).await, Err(WaitError::NotFound)));
    }

    #[tokio::test]
    async fn a_rejection_says_until_when_the_request_is_denied() {
        let mut rejected = challenge(ChallengeStatus::Rejected);
        rejected.expires_at = Some(awzy_authz_proto::prost_types::Timestamp { seconds: 1_800_000_000, nanos: 0 });
        let get = || {
            let c = rejected.clone();
            async move { Ok(Some(c)) }
        };
        let res = wait(&bot(), Some(&dana()), Duration::from_secs(25), get).await.unwrap();
        assert_eq!(res.outcome(), ChallengeOutcome::Rejected);
        assert_eq!(res.expires_at.unwrap().seconds, 1_800_000_000);
    }
}
