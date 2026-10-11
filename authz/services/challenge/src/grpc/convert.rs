//! Between the wire and the store: checking requests' fields, and errors and records as gRPC
//! returns them.

use super::*;

pub(super) fn qid<T: std::str::FromStr<Err = awzy_model::QidError>>(s: &str) -> Result<T, Status> {
    s.parse().map_err(|e: awzy_model::QidError| Status::invalid_argument(e.to_string()))
}

pub(super) fn required(s: String, what: &str) -> Result<String, Status> {
    if s.is_empty() {
        Err(Status::invalid_argument(format!("{what} is required")))
    } else {
        Ok(s)
    }
}

pub(super) fn assertion(a: WireAssertion) -> Result<Assertion, Status> {
    Ok(Assertion {
        ceremony_id: qid(&a.ceremony_qid)?,
        credential_id: a.credential_id,
        client_data_json: a.client_data_json,
        authenticator_data: a.authenticator_data,
        signature: a.signature,
    })
}

pub(super) fn key_status(e: KeyError) -> Status {
    match e {
        KeyError::Challenge(e) => status(e),
        KeyError::WebAuthn(_) | KeyError::UnknownKey { .. } | KeyError::NeedsExistingKey { .. } => {
            Status::permission_denied(e.to_string())
        }
        KeyError::NoKeys { .. } | KeyError::BadCeremony => Status::failed_precondition(e.to_string()),
        KeyError::NotFound(_) => Status::not_found(e.to_string()),
        KeyError::AlreadyRegistered => Status::already_exists(e.to_string()),
        KeyError::Invalid(_) => Status::invalid_argument(e.to_string()),
        KeyError::Db(error) => {
            tracing::error!(%error, "security key store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

pub(super) fn key_to_wire(k: SecurityKey) -> WireKey {
    let (removed_by_actor, removed_by) = awzy_authz_proto::actor_to_wire(k.removed_by.as_deref().unwrap_or(""));
    WireKey {
        qid: k.id.to_string(),
        human_id: k.human_id,
        name: k.name,
        added_with_qid: k.added_with.map(|id| id.to_string()).unwrap_or_default(),
        created_at: Some(ts(k.created_at)),
        last_used_at: k.last_used_at.map(ts),
        removed_at: k.removed_at.map(ts),
        removed_by,
        removed_by_actor,
    }
}

pub(super) fn required_ref(r: Option<awzy_authz_proto::authz::v1::EntityRef>, what: &str) -> Result<EntityRef, Status> {
    match r {
        Some(r) if !r.r#type.is_empty() && !r.id.is_empty() => Ok(r.into()),
        _ => Err(Status::invalid_argument(format!("{what} is required"))),
    }
}

pub(super) fn limit(n: i32) -> i64 {
    if n <= 0 {
        50
    } else {
        n.min(500).into()
    }
}

pub(super) fn status(e: StoreError) -> Status {
    match e {
        StoreError::ChallengeNotFound(_) | StoreError::ContainmentNotFound(_) => Status::not_found(e.to_string()),
        StoreError::NotPending { .. }
        | StoreError::KindNotRequired { .. }
        | StoreError::AlreadyPassed { .. }
        | StoreError::AlreadyLifted(_) => Status::failed_precondition(e.to_string()),
        StoreError::NotAllowed(_) => Status::permission_denied(e.to_string()),
        StoreError::Invalid(_) => Status::invalid_argument(e.to_string()),
        StoreError::Db(error) => {
            tracing::error!(%error, "challenge store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

pub(super) fn support_status(e: SupportError) -> Status {
    match e {
        SupportError::NotFound(_) => Status::not_found(e.to_string()),
        SupportError::Invalid(_) => Status::invalid_argument(e.to_string()),
        SupportError::WrongStatus { .. } => Status::failed_precondition(e.to_string()),
        SupportError::Db(error) => {
            tracing::error!(%error, "support store failed");
            Status::internal("challenge store unavailable")
        }
    }
}

/// Who acted, stored as text in its typed form (ADR 0060).
pub(super) fn actor(
    actor: Option<awzy_authz_proto::authz::v1::Actor>,
    text: &str,
    field: &str,
) -> Result<String, Status> {
    awzy_authz_proto::actor_from_wire(actor, text)
        .map(|a| a.to_string())
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

/// The person who answered or decided, by their IdP ID: only a person, acting for themselves,
/// answers for a challenge, and approvals are checked against them as a Cedar principal.
pub(super) fn human(
    actor: Option<awzy_authz_proto::authz::v1::Actor>,
    text: &str,
    field: &str,
) -> Result<String, Status> {
    match awzy_authz_proto::actor_from_wire(actor, text) {
        Some(a) if a.subject.kind == awzy_model::entity_type::HUMAN && a.on_behalf_of.is_none() => Ok(a.subject.id),
        Some(a) => Err(Status::invalid_argument(format!("{field} must be a person acting for themselves, not {a}"))),
        None => Err(Status::invalid_argument(format!("{field} is required"))),
    }
}

/// Who may approve: a person, or a service account such as an overseer agent (ADR 0065), acting on
/// its own authority. Policy decides whether it may approve this resource.
pub(super) fn approver(
    actor: Option<awzy_authz_proto::authz::v1::Actor>,
    text: &str,
) -> Result<awzy_model::Actor, Status> {
    use awzy_model::entity_type::{HUMAN, SERVICE_ACCOUNT};
    match awzy_authz_proto::actor_from_wire(actor, text) {
        Some(a) if [HUMAN, SERVICE_ACCOUNT].contains(&a.subject.kind.as_str()) && a.on_behalf_of.is_none() => Ok(a),
        Some(a) => Err(Status::invalid_argument(format!(
            "decided_by_actor must be a person or a service account acting on its own authority, not {a}"
        ))),
        None => Err(Status::invalid_argument("decided_by_actor is required")),
    }
}

pub(super) fn ts(t: DateTime<Utc>) -> Timestamp {
    Timestamp { seconds: t.timestamp(), nanos: t.timestamp_subsec_nanos() as i32 }
}

pub(super) fn challenge_to_wire(c: Challenge) -> WireChallenge {
    let (decided_by_actor, decided_by) = awzy_authz_proto::actor_to_wire(c.decided_by.as_deref().unwrap_or(""));
    WireChallenge {
        qid: c.id.to_string(),
        decision_qid: c.decision_id.to_string(),
        principal: Some(c.request.principal.into()),
        on_behalf_of: c.request.on_behalf_of.map(Into::into),
        action: c.request.action,
        resource: Some(c.request.resource.into()),
        kinds: challenges_to_wire(c.kinds),
        passed: challenges_to_wire(c.passed),
        status: match c.status {
            ChallengeStatus::Pending => WireStatus::Pending,
            ChallengeStatus::Passed => WireStatus::Passed,
            ChallengeStatus::Rejected => WireStatus::Rejected,
            ChallengeStatus::Expired => WireStatus::Expired,
        }
        .into(),
        answerer_id: c.answerer_id.unwrap_or_default(),
        justification: c.justification.unwrap_or_default(),
        decided_by,
        decided_by_actor,
        decision_note: c.decision_note.unwrap_or_default(),
        created_at: Some(ts(c.created_at)),
        expires_at: Some(ts(c.expires_at)),
        passed_at: c.passed_at.map(ts),
        grant_expires_at: c.grant_expires_at.map(ts),
    }
}

pub(super) fn containment_to_wire(c: Containment) -> WireContainment {
    let (created_by_actor, created_by) = awzy_authz_proto::actor_to_wire(&c.created_by);
    let (lifted_by_actor, lifted_by) = awzy_authz_proto::actor_to_wire(c.lifted_by.as_deref().unwrap_or(""));
    WireContainment {
        qid: c.id.to_string(),
        principal: Some(c.principal.into()),
        reason: c.reason,
        created_by,
        created_by_actor,
        created_at: Some(ts(c.created_at)),
        lifted_at: c.lifted_at.map(ts),
        lifted_by,
        lifted_by_actor,
        lift_reason: c.lift_reason.unwrap_or_default(),
        monitor: c.monitor,
    }
}
