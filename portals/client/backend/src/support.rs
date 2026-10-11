//! Support sessions (ADR 0095): an awzy staff member views named areas of this portal, read-only,
//! while a session a client admin approved lasts.
//!
//! The staff member signs in with a short-lived token the corp portal signs for one session and
//! this install. The portal accepts it only while awzy-challenge says that session is approved and
//! unexpired, for that staff member, and checks each of their operations twice: here (only `view`,
//! only the approved areas) and with the PDP, whose base policies say the same. Each operation is
//! recorded against the session.

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute},
    Context, Enum, InputObject, Object, Response, Result, SimpleObject, ID,
};
use awzy_authz_proto::challenge::v1::{
    DecideSupportSessionRequest, EndSupportSessionRequest, GetSupportSessionRequest, ListSupportSessionsRequest,
    RecordSupportActionRequest, SupportSession as WireSession, SupportSessionStatus as WireStatus,
};
use awzy_identity::Principal;
use awzy_model::SupportSessionId;
use awzy_portal::{Actor, UtcTimestamp};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;

use crate::{
    access::{require, PortalAction::*, PortalArea, Viewer},
    challenges::{client, grpc, ChallengeClient},
    enforcement::switch_challenge,
};

/// Who signs support tokens: the corp portal.
pub const ISSUER: &str = "awzy-corp-portal";

/// What an approved session lets a staff member do: view these areas until then.
#[derive(Debug, Clone, PartialEq)]
pub struct SupportGrant {
    pub session: SupportSessionId,
    pub staff: String,
    pub areas: Vec<String>,
    pub expires_at: DateTime<Utc>,
}

impl SupportGrant {
    /// Whether the staff member may do `action` in `area`: only view, only an approved area.
    pub fn allows(&self, action: &str, area: PortalArea) -> bool {
        action == "view" && self.areas.iter().any(|a| a == area.as_str())
    }
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    /// The session's QID.
    sid: String,
}

/// Signing in with the corp portal's support tokens.
pub struct SupportSignIn {
    key: DecodingKey,
    /// `awzy-support:<install QID>`, so a token for another install doesn't work here.
    audience: String,
    challenge: ChallengeClient,
}

impl SupportSignIn {
    /// From AWZY_SUPPORT_PUBLIC_KEY_FILE (the corp portal's support key, PEM) and AWZY_INSTALL_ID.
    /// None without the key: then support sessions can't sign in.
    pub fn from_env(challenge: Option<ChallengeClient>) -> anyhow::Result<Option<Self>> {
        let Ok(path) = std::env::var("AWZY_SUPPORT_PUBLIC_KEY_FILE") else { return Ok(None) };
        let install = std::env::var("AWZY_INSTALL_ID")
            .map_err(|_| anyhow::anyhow!("AWZY_INSTALL_ID is required with AWZY_SUPPORT_PUBLIC_KEY_FILE"))?;
        let challenge =
            challenge.ok_or_else(|| anyhow::anyhow!("support sessions need awzy-challenge (AWZY_CHALLENGE_ADDR)"))?;
        let key = DecodingKey::from_ec_pem(&std::fs::read(&path)?)?;
        Ok(Some(Self::new(key, &install, challenge)))
    }

    pub fn new(key: DecodingKey, install: &str, challenge: ChallengeClient) -> Self {
        Self { key, audience: format!("awzy-support:{install}"), challenge }
    }

    /// Whether `token` is a support token at all: the corp portal's, not the client's IdP's.
    pub fn claims_it(token: &str) -> bool {
        let Some(payload) = token.split('.').nth(1) else { return false };
        use base64::Engine;
        let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload) else { return false };
        serde_json::from_slice::<serde_json::Value>(&bytes).is_ok_and(|c| c["iss"] == ISSUER)
    }

    /// The viewer a support token names, if it's valid and its session is approved now.
    pub async fn viewer(&self, token: &str) -> Result<Viewer, String> {
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_issuer(&[ISSUER]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = jsonwebtoken::decode::<Claims>(token, &self.key, &validation)
            .map_err(|e| format!("support token refused: {e}"))?
            .claims;
        let mut challenge = self.challenge.clone();
        let session = challenge
            .get_support_session(GetSupportSessionRequest { qid: claims.sid.clone() })
            .await
            .map_err(|e| format!("support session {}: {}", claims.sid, e.message()))?
            .into_inner()
            .session
            .ok_or("awzy-challenge returned no support session")?;
        let grant = grant(&session, &claims.sub)?;
        Ok(viewer_for(grant))
    }
}

/// The grant an approved session gives `staff`, or why there's none.
fn grant(session: &WireSession, staff: &str) -> Result<SupportGrant, String> {
    let request = session.request.as_ref().ok_or("a support session without its request")?;
    if session.status() != WireStatus::Approved {
        return Err(format!("support session {} isn't approved now", request.qid));
    }
    if request.requested_by != staff {
        return Err(format!("support session {} is another staff member's", request.qid));
    }
    let expires_at = session
        .expires_at
        .as_ref()
        .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
        .filter(|t| *t > Utc::now())
        .ok_or_else(|| format!("support session {} has ended", request.qid))?;
    Ok(SupportGrant {
        session: request.qid.parse().map_err(|e: awzy_model::QidError| e.to_string())?,
        staff: staff.to_string(),
        areas: session.approved_areas.clone(),
        expires_at,
    })
}

/// A staff member as a principal: a person the directory doesn't know, in no group, so only the
/// support policies apply to them.
fn viewer_for(grant: SupportGrant) -> Viewer {
    let subject = awzy_model::EntityRef::new(awzy_model::entity_type::HUMAN, format!("awzy-support:{}", grant.staff));
    let principal = Principal {
        entities: vec![serde_json::json!({
            "uid": { "type": subject.kind, "id": subject.id },
            "attrs": {},
            "parents": [],
        })],
        subject,
        on_behalf_of: None,
        human_present: Some(true),
        name: Some(format!("{} (awzy support)", grant.staff)),
        expires_at: Some(
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(grant.expires_at.timestamp().max(0) as u64),
        ),
    };
    Viewer(Arc::new(principal), Some(Arc::new(grant)))
}

// ---- Recording what a staff member does

/// Records each operation a support viewer runs against their session (ADR 0095). A recording
/// that fails stops the operation: a session that can't be audited doesn't run.
pub struct SupportAudit;

impl ExtensionFactory for SupportAudit {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(SupportAudit)
    }
}

#[async_trait::async_trait]
impl Extension for SupportAudit {
    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let grant = ctx.data_opt::<Viewer>().and_then(|v| v.1.clone());
        if let Some(grant) = grant {
            let Some(mut challenge) = ctx.data_opt::<crate::Services>().and_then(|s| s.challenge.clone()) else {
                return Response::from_errors(vec![async_graphql::ServerError::new(
                    "awzy-challenge is not configured",
                    None,
                )]);
            };
            let operation = operation_name.unwrap_or("anonymous").to_string();
            let req = RecordSupportActionRequest { qid: grant.session.to_string(), operation: operation.clone() };
            if let Err(e) = challenge.record_support_action(req).await {
                tracing::error!(session = %grant.session, %operation, error = %e.message(), "couldn't record a support action");
                return Response::from_errors(vec![async_graphql::ServerError::new(
                    "this support session's actions can't be recorded right now, so nothing ran",
                    None,
                )]);
            }
            tracing::info!(session = %grant.session, staff = %grant.staff, %operation, "support action");
        }
        next.run(ctx, operation_name).await
    }
}

// ---- GraphQL

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum SupportSessionStatus {
    Requested,
    Approved,
    Rejected,
    Ended,
    Expired,
}

impl From<WireStatus> for SupportSessionStatus {
    fn from(s: WireStatus) -> Self {
        match s {
            WireStatus::Approved => Self::Approved,
            WireStatus::Rejected => Self::Rejected,
            WireStatus::Ended => Self::Ended,
            WireStatus::Expired => Self::Expired,
            WireStatus::Requested | WireStatus::Unspecified => Self::Requested,
        }
    }
}

fn time(t: &Option<awzy_authz_proto::prost_types::Timestamp>) -> Option<UtcTimestamp> {
    t.as_ref().and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32)).map(UtcTimestamp)
}

fn actor(a: &Option<awzy_authz_proto::authz::v1::Actor>) -> Option<Actor> {
    awzy_authz_proto::actor_from_wire(a.clone(), "").and_then(|a| Actor::from_text(&a.to_string()))
}

/// One support session, as an admin sees it.
pub struct SupportSession(WireSession);

#[Object]
impl SupportSession {
    /// Its QID, which the corp portal minted.
    async fn id(&self) -> ID {
        ID(self.0.request.as_ref().map(|r| r.qid.clone()).unwrap_or_default())
    }

    async fn status(&self) -> SupportSessionStatus {
        self.0.status().into()
    }

    /// The awzy staff member who asked.
    async fn requested_by(&self) -> &str {
        self.0.request.as_ref().map_or("", |r| r.requested_by.as_str())
    }

    /// The portal areas asked for.
    async fn areas(&self) -> Vec<String> {
        self.0.request.as_ref().map(|r| r.areas.clone()).unwrap_or_default()
    }

    async fn reason(&self) -> &str {
        self.0.request.as_ref().map_or("", |r| r.reason.as_str())
    }

    /// How long was asked for.
    async fn duration_minutes(&self) -> u32 {
        self.0.request.as_ref().map_or(0, |r| r.duration_minutes)
    }

    async fn requested_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.request.as_ref().and_then(|r| r.requested_at))
    }

    /// The areas approved, at most those asked for.
    async fn approved_areas(&self) -> &[String] {
        &self.0.approved_areas
    }

    async fn expires_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.expires_at)
    }

    async fn decided_by(&self) -> Option<Actor> {
        actor(&self.0.decided_by_actor)
    }

    async fn decided_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.decided_at)
    }

    async fn ended_by(&self) -> Option<Actor> {
        actor(&self.0.ended_by_actor)
    }

    async fn ended_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.ended_at)
    }

    /// How many operations the staff member has made in it.
    async fn action_count(&self) -> u32 {
        self.0.action_count
    }
}

/// The signed-in viewer's own support session, when they're awzy staff in one.
#[derive(SimpleObject)]
pub struct ViewerSupport {
    pub support_session_qid: ID,
    pub staff: String,
    pub areas: Vec<String>,
    pub expires_at: UtcTimestamp,
}

/// One support session, for `node(id)`. For those who may view enforcement.
pub(crate) async fn get_by_id(ctx: &Context<'_>, id: SupportSessionId) -> Result<Option<SupportSession>> {
    require(ctx, View, PortalArea::Enforcement).await?;
    match client(ctx)?.get_support_session(GetSupportSessionRequest { qid: id.to_string() }).await {
        Ok(res) => Ok(res.into_inner().session.map(SupportSession)),
        Err(s) if s.code() == tonic::Code::NotFound => Ok(None),
        Err(s) => Err(grpc(s)),
    }
}

#[derive(Default)]
pub struct SupportQuery;

#[Object]
impl SupportQuery {
    /// Support sessions awzy staff asked for, newest first. For those who may view enforcement.
    async fn support_sessions(&self, ctx: &Context<'_>) -> Result<Vec<SupportSession>> {
        require(ctx, View, PortalArea::Enforcement).await?;
        let res = client(ctx)?.list_support_sessions(ListSupportSessionsRequest { limit: 50 }).await.map_err(grpc)?;
        Ok(res.into_inner().sessions.into_iter().map(SupportSession).collect())
    }

    /// Null unless the viewer is awzy staff in a support session.
    async fn viewer_support(&self, ctx: &Context<'_>) -> Option<ViewerSupport> {
        let grant = ctx.data_opt::<Viewer>()?.1.clone()?;
        Some(ViewerSupport {
            support_session_qid: ID(grant.session.to_string()),
            staff: grant.staff.clone(),
            areas: grant.areas.clone(),
            expires_at: UtcTimestamp(grant.expires_at),
        })
    }
}

#[derive(InputObject)]
pub struct DecideSupportSessionInput {
    pub support_session_qid: ID,
    pub approve: bool,
    /// With approve: the areas to allow, at most those asked for.
    #[graphql(default)]
    pub areas: Vec<String>,
    /// With approve: for how long, at most what was asked for.
    #[graphql(default)]
    pub minutes: u32,
}

#[derive(SimpleObject)]
pub struct DecideSupportSessionPayload {
    /// The session as decided; null when a challenge must be passed first.
    pub support_session: Option<SupportSession>,
    /// The challenge to pass, then decide again.
    pub challenge_qid: Option<ID>,
}

#[derive(Default)]
pub struct SupportMutation;

#[Object]
impl SupportMutation {
    /// Approves or rejects a support session. Approving lets an awzy staff member view the areas
    /// approved, read-only, until it expires; like switching enforcement, policy can ask for a
    /// security key first.
    async fn decide_support_session(
        &self,
        ctx: &Context<'_>,
        input: DecideSupportSessionInput,
    ) -> Result<DecideSupportSessionPayload> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        if input.approve {
            if let Some(challenge_qid) = switch_challenge(ctx, &viewer).await? {
                return Ok(DecideSupportSessionPayload { support_session: None, challenge_qid: Some(challenge_qid) });
            }
        }
        let req = DecideSupportSessionRequest {
            qid: input.support_session_qid.0,
            approve: input.approve,
            areas: input.areas,
            minutes: input.minutes,
            decided_by_actor: Some(viewer.actor().into()),
        };
        let session = client(ctx)?.decide_support_session(req).await.map_err(grpc)?.into_inner().session;
        Ok(DecideSupportSessionPayload { support_session: session.map(SupportSession), challenge_qid: None })
    }

    /// Ends an approved support session now. The staff member's next request is refused.
    async fn end_support_session(&self, ctx: &Context<'_>, support_session_qid: ID) -> Result<SupportSession> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        let req = EndSupportSessionRequest { qid: support_session_qid.0, ended_by_actor: Some(viewer.actor().into()) };
        let session = client(ctx)?.end_support_session(req).await.map_err(grpc)?.into_inner().session;
        Ok(SupportSession(session.ok_or("awzy-challenge returned no support session")?))
    }
}

#[cfg(test)]
mod tests {
    use awzy_authz_proto::challenge::v1::SupportRequest;
    use awzy_authz_proto::prost_types::Timestamp;

    use super::*;

    fn session(status: WireStatus, by: &str, minutes_left: i64) -> WireSession {
        let expires = Utc::now() + chrono::Duration::minutes(minutes_left);
        WireSession {
            request: Some(SupportRequest {
                qid: SupportSessionId::new().to_string(),
                requested_by: by.into(),
                areas: vec!["decisions".into(), "policies".into()],
                ..Default::default()
            }),
            status: status.into(),
            approved_areas: vec!["decisions".into()],
            expires_at: Some(Timestamp { seconds: expires.timestamp(), nanos: 0 }),
            ..Default::default()
        }
    }

    #[test]
    fn only_an_approved_unexpired_session_of_that_staff_member_signs_in() {
        let ok = grant(&session(WireStatus::Approved, "max", 30), "max").unwrap();
        assert!(ok.allows("view", PortalArea::Decisions));
        assert!(!ok.allows("view", PortalArea::Policies), "asked for, not approved");
        assert!(!ok.allows("administer", PortalArea::Decisions));
        assert!(grant(&session(WireStatus::Requested, "max", 30), "max").is_err());
        assert!(grant(&session(WireStatus::Ended, "max", 30), "max").is_err());
        assert!(grant(&session(WireStatus::Approved, "max", -1), "max").is_err(), "expired");
        assert!(grant(&session(WireStatus::Approved, "other", 30), "max").is_err());
    }

    #[test]
    fn a_support_token_is_told_apart_from_the_idps() {
        use base64::Engine;
        let enc = |v: serde_json::Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
        let token = |iss: &str| {
            format!("{}.{}.sig", enc(serde_json::json!({ "alg": "ES256" })), enc(serde_json::json!({ "iss": iss })))
        };
        assert!(SupportSignIn::claims_it(&token(ISSUER)));
        assert!(!SupportSignIn::claims_it(&token("https://idp.example.com")));
        assert!(!SupportSignIn::claims_it("not-a-jwt"));
    }
}
