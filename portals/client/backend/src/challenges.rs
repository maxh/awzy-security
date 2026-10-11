//! Challenges and containments (ADR 0054), through awzy-challenge.
//!
//! - Anyone who may `view` the challenges area answers the challenges sent to them, and may
//!   approve someone else's request where policy lets them (awzy-challenge asks the PDP).
//! - Those who may `administer` it see every challenge and contain or lift principals.
//!
//! Only a person answers: a service account signed in, or acting for someone, is refused, so an
//! agent can never pass its own challenge. A service account may approve on its own authority
//! where policy lets it (ADR 0065), never while acting for someone, and never its own request.

use std::time::{Duration, SystemTime};

use async_graphql::{
    futures_util::{Stream, StreamExt},
    Context, Enum, InputObject, Object, Result, SimpleObject, Subscription, ID,
};
use awzy_authz_proto::{
    challenge::v1::{
        challenge_service_client::ChallengeServiceClient, Challenge as WireChallenge, ChallengeStatus as WireStatus,
        ContainRequest, Containment as WireContainment, DecideRequest, GetChallengeRequest, JustifyRequest,
        LiftRequest, ListChallengesRequest, ListContainmentsRequest, WatchChallengesRequest, WatchContainmentsRequest,
    },
    challenges_from_wire,
    prost_types::Timestamp,
};
use awzy_model::{entity_type, ChallengeId, ContainmentId};
use awzy_portal::{Actor, UtcTimestamp};
use chrono::DateTime;

use crate::{
    access::{authorize, require, PortalAction, PortalArea, Surface, Viewer},
    decisions::EntityRef,
    Services,
};

pub(crate) type ChallengeClient = ChallengeServiceClient<awzy_service_auth::AuthChannel>;

pub(crate) fn client(ctx: &Context<'_>) -> Result<ChallengeClient> {
    ctx.data::<Services>()
        .ok()
        .and_then(|s| s.challenge.clone())
        .ok_or_else(|| "awzy-challenge is not configured (AWZY_CHALLENGE_ADDR)".into())
}

/// awzy-challenge's own message, without tonic's wrapping.
pub(crate) fn grpc(s: tonic::Status) -> async_graphql::Error {
    s.message().to_string().into()
}

/// The signed-in person, or an error for a service account.
pub(crate) fn answering_human(viewer: &Viewer) -> Result<String> {
    if viewer.subject.kind != entity_type::HUMAN || viewer.on_behalf_of.is_some() {
        return Err(
            format!("{} can't answer challenges; only the person can, in the portal", viewer.actor().label()).into()
        );
    }
    Ok(viewer.subject.id.clone())
}

pub(crate) fn time(t: &Option<Timestamp>) -> Option<UtcTimestamp> {
    t.as_ref().and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32)).map(UtcTimestamp)
}

fn entity(r: &Option<awzy_authz_proto::authz::v1::EntityRef>) -> Option<EntityRef> {
    r.as_ref().map(|r| EntityRef { kind: r.r#type.clone(), id: r.id.clone() })
}

fn first(n: Option<i32>) -> i32 {
    n.unwrap_or(50).clamp(1, 500)
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ChallengeKind {
    /// The person says why they need the access.
    Justification,
    /// A security key (WebAuthn). Not available yet.
    Verification,
    /// Someone else approves.
    Approval,
    /// A biometric check through an external provider. Not available yet.
    Biometric,
}

fn kinds(wire: &[i32]) -> Vec<ChallengeKind> {
    challenges_from_wire(wire)
        .iter()
        .map(|k| match k {
            awzy_model::ChallengeKind::Justification => ChallengeKind::Justification,
            awzy_model::ChallengeKind::Verification => ChallengeKind::Verification,
            awzy_model::ChallengeKind::Approval => ChallengeKind::Approval,
            awzy_model::ChallengeKind::Biometric => ChallengeKind::Biometric,
        })
        .collect()
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ChallengeStatus {
    /// Waiting for an answer.
    Pending,
    /// Every kind passed: the request is allowed on retry until `grantExpiresAt`.
    Passed,
    /// An approver said no: the request is denied until `expiresAt`.
    Rejected,
    /// Not answered in time.
    Expired,
}

impl From<ChallengeStatus> for WireStatus {
    fn from(s: ChallengeStatus) -> Self {
        match s {
            ChallengeStatus::Pending => WireStatus::Pending,
            ChallengeStatus::Passed => WireStatus::Passed,
            ChallengeStatus::Rejected => WireStatus::Rejected,
            ChallengeStatus::Expired => WireStatus::Expired,
        }
    }
}

/// One challenge: what a request must pass before it's allowed.
pub struct Challenge(pub WireChallenge);

#[Object]
impl Challenge {
    /// QID: challenge_<uuid>.
    pub async fn id(&self) -> ID {
        ID(self.0.qid.clone())
    }

    /// The decision that opened it.
    async fn decision_qid(&self) -> ID {
        ID(self.0.decision_qid.clone())
    }

    async fn principal(&self) -> Option<EntityRef> {
        entity(&self.0.principal)
    }

    /// Set when a service account acted for a person.
    async fn on_behalf_of(&self) -> Option<EntityRef> {
        entity(&self.0.on_behalf_of)
    }

    async fn action(&self) -> &str {
        &self.0.action
    }

    async fn resource(&self) -> Option<EntityRef> {
        entity(&self.0.resource)
    }

    /// Every one must be passed.
    async fn kinds(&self) -> Vec<ChallengeKind> {
        kinds(&self.0.kinds)
    }

    async fn passed(&self) -> Vec<ChallengeKind> {
        kinds(&self.0.passed)
    }

    async fn status(&self) -> ChallengeStatus {
        match self.0.status() {
            WireStatus::Passed => ChallengeStatus::Passed,
            WireStatus::Rejected => ChallengeStatus::Rejected,
            WireStatus::Expired => ChallengeStatus::Expired,
            WireStatus::Pending | WireStatus::Unspecified => ChallengeStatus::Pending,
        }
    }

    /// The person it was sent to. Null when no one is known, so only an approval can pass it.
    async fn answerer_id(&self) -> Option<&str> {
        Some(self.0.answerer_id.as_str()).filter(|s| !s.is_empty())
    }

    async fn justification(&self) -> Option<&str> {
        Some(self.0.justification.as_str()).filter(|s| !s.is_empty())
    }

    /// Who approved or rejected it.
    async fn decided_by(&self) -> Option<Actor> {
        Actor::from_text(&self.0.decided_by)
    }

    async fn decision_note(&self) -> Option<&str> {
        Some(self.0.decision_note.as_str()).filter(|s| !s.is_empty())
    }

    async fn created_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.created_at)
    }

    /// A pending challenge expires then; a rejection stops blocking the request then.
    async fn expires_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.expires_at)
    }

    async fn passed_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.passed_at)
    }

    /// A passed challenge allows the request until then.
    async fn grant_expires_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.grant_expires_at)
    }
}

/// A principal whose access is revoked until a person lifts it.
pub struct Containment(pub WireContainment);

#[Object]
impl Containment {
    /// QID: containment_<uuid>.
    async fn id(&self) -> ID {
        ID(self.0.qid.clone())
    }

    async fn principal(&self) -> Option<EntityRef> {
        entity(&self.0.principal)
    }

    async fn reason(&self) -> &str {
        &self.0.reason
    }

    async fn created_by(&self) -> Option<Actor> {
        Actor::from_text(&self.0.created_by)
    }

    async fn created_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.created_at)
    }

    /// Null while it's in force.
    async fn lifted_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.lifted_at)
    }

    async fn lifted_by(&self) -> Option<Actor> {
        Actor::from_text(&self.0.lifted_by)
    }

    async fn lift_reason(&self) -> Option<&str> {
        Some(self.0.lift_reason.as_str()).filter(|s| !s.is_empty())
    }
    /// Recorded but not applied: awzy-reasoner would have contained this principal, and automated
    /// containments are in monitor (ADR 0066). Lifting it dismisses it.
    async fn monitor(&self) -> bool {
        self.0.monitor
    }
}

/// For `node(id)`: the challenge, if the viewer is the person it was sent to or administers
/// challenges.
/// One containment, for `node(id)`. For those who administer challenges. awzy-challenge has no
/// lookup by ID, so this looks through the most recent ones, lifted included.
pub(crate) async fn containment_by_id(ctx: &Context<'_>, id: ContainmentId) -> Result<Option<Containment>> {
    require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
    let req = ListContainmentsRequest { active_only: false, principal: None, limit: 500, include_monitor: true };
    let list = client(ctx)?.list_containments(req).await.map_err(grpc)?.into_inner().containments;
    let qid = id.to_string();
    Ok(list.into_iter().find(|c| c.qid == qid).map(Containment))
}

pub(crate) async fn get_by_id(ctx: &Context<'_>, id: ChallengeId) -> Result<Option<Challenge>> {
    let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
    let res = client(ctx)?.get_challenge(GetChallengeRequest { challenge_qid: id.to_string() }).await;
    let c = match res {
        Ok(r) => r.into_inner().challenge,
        Err(s) if s.code() == tonic::Code::NotFound => None,
        Err(s) => return Err(grpc(s)),
    };
    let Some(c) = c else { return Ok(None) };
    let answers_it = c.answerer_id == viewer.subject.id && viewer.subject.kind == entity_type::HUMAN;
    if !answers_it && !may_approve(ctx.data::<Services>()?, &viewer, &c).await {
        require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
    }
    Ok(Some(Challenge(c)))
}

/// Whether `viewer` may approve challenge `c` now: it's waiting for an approval, the viewer isn't
/// a party to it, and policy lets them `approve` its resource. The same check awzy-challenge makes
/// before it records an approval, asked ahead so the portal shows each approver what they can
/// act on. Any failure reads as no.
pub(crate) async fn may_approve(services: &Services, viewer: &Viewer, c: &WireChallenge) -> bool {
    use awzy_authz_proto::authz::v1::{CheckRequest, Verdict};
    let approval = awzy_authz_proto::authz::v1::ChallengeKind::Approval as i32;
    let pending = c.status() == WireStatus::Pending && c.kinds.contains(&approval) && !c.passed.contains(&approval);
    let (Some(mut pdp), Some(resource)) = (services.pdp.clone(), c.resource.clone()) else { return false };
    if !pending || viewer.on_behalf_of.is_some() {
        return false;
    }
    let me = |r: &Option<awzy_authz_proto::authz::v1::EntityRef>| {
        r.as_ref().is_some_and(|r| r.r#type == viewer.subject.kind && r.id == viewer.subject.id)
    };
    let answers = viewer.subject.kind == entity_type::HUMAN && c.answerer_id == viewer.subject.id;
    if me(&c.principal) || me(&c.on_behalf_of) || answers {
        return false;
    }
    let mut context = serde_json::Map::new();
    if !c.answerer_id.is_empty() {
        context.insert(
            "approving_for".into(),
            serde_json::json!({ "__entity": { "type": entity_type::HUMAN, "id": c.answerer_id } }),
        );
    }
    let req = CheckRequest {
        principal: Some(awzy_authz_proto::authz::v1::EntityRef {
            r#type: viewer.subject.kind.clone(),
            id: viewer.subject.id.clone(),
        }),
        action: "approve".into(),
        resource: Some(resource),
        context: Some(awzy_authz_proto::json_to_struct(context)),
        entities: viewer.entities.iter().map(awzy_authz_proto::entity_from_json).collect(),
        always_enforced: true,
        ..Default::default()
    };
    matches!(pdp.check(req).await, Ok(res) if res.get_ref().verdict() == Verdict::Allow)
}

#[derive(Default)]
pub struct ChallengeQuery;

#[Object]
impl ChallengeQuery {
    /// Challenges sent to the signed-in person, newest first.
    async fn my_challenges(
        &self,
        ctx: &Context<'_>,
        status: Option<ChallengeStatus>,
        first: Option<i32>,
    ) -> Result<Vec<Challenge>> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let me = answering_human(&viewer)?;
        list(ctx, me, status, first).await
    }

    /// Requests waiting for an approval the signed-in principal may give, newest first: a
    /// resource's owner, say, or an overseer agent policy names.
    async fn challenges_to_approve(&self, ctx: &Context<'_>, first: Option<i32>) -> Result<Vec<Challenge>> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let services = ctx.data::<Services>()?;
        if viewer.on_behalf_of.is_some() {
            return Ok(vec![]);
        }
        let pending = list(ctx, String::new(), Some(ChallengeStatus::Pending), Some(500)).await?;
        let checks = pending.iter().map(|c| may_approve(services, &viewer, &c.0));
        let allowed = async_graphql::futures_util::future::join_all(checks).await;
        let n = self::first(first) as usize;
        Ok(pending.into_iter().zip(allowed).filter(|(_, ok)| *ok).map(|(c, _)| c).take(n).collect())
    }

    /// Every challenge, newest first. For those who administer challenges.
    async fn challenges(
        &self,
        ctx: &Context<'_>,
        status: Option<ChallengeStatus>,
        first: Option<i32>,
    ) -> Result<Vec<Challenge>> {
        require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        list(ctx, String::new(), status, first).await
    }

    /// Containments, newest first. For those who administer challenges.
    async fn containments(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = true)] active_only: bool,
        first: Option<i32>,
    ) -> Result<Vec<Containment>> {
        require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let req =
            ListContainmentsRequest { active_only, principal: None, limit: self::first(first), include_monitor: true };
        let list = client(ctx)?.list_containments(req).await.map_err(grpc)?.into_inner().containments;
        Ok(list.into_iter().map(Containment).collect())
    }
}

async fn list(
    ctx: &Context<'_>,
    answerer_id: String,
    status: Option<ChallengeStatus>,
    n: Option<i32>,
) -> Result<Vec<Challenge>> {
    let req = ListChallengesRequest {
        answerer_id,
        status: status.map_or(WireStatus::Unspecified, Into::into).into(),
        limit: first(n),
    };
    let list = client(ctx)?.list_challenges(req).await.map_err(grpc)?.into_inner().challenges;
    Ok(list.into_iter().map(Challenge).collect())
}

#[derive(InputObject)]
pub struct JustifyChallengeInput {
    pub challenge_qid: ID,
    /// Why the access is needed: at least 20 characters.
    pub justification: String,
}

#[derive(InputObject)]
pub struct DecideChallengeInput {
    pub challenge_qid: ID,
    pub approve: bool,
    pub note: Option<String>,
}

#[derive(SimpleObject)]
pub struct ChallengePayload {
    pub challenge: Challenge,
}

#[derive(InputObject)]
pub struct EntityRefInput {
    /// `Human` or `ServiceAccount`.
    #[graphql(name = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(InputObject)]
pub struct ContainPrincipalInput {
    pub principal: EntityRefInput,
    pub reason: String,
}

#[derive(InputObject)]
pub struct LiftContainmentInput {
    pub containment_qid: ID,
    pub reason: String,
}

#[derive(SimpleObject)]
pub struct ContainmentPayload {
    pub containment: Containment,
}

#[derive(Default)]
pub struct ChallengeMutation;

#[Object]
impl ChallengeMutation {
    /// Answers a justification challenge sent to the signed-in person.
    async fn justify_challenge(&self, ctx: &Context<'_>, input: JustifyChallengeInput) -> Result<ChallengePayload> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = JustifyRequest {
            challenge_qid: input.challenge_qid.0,
            answered_by_actor: Some(awzy_model::Actor::human(answering_human(&viewer)?).into()),
            justification: input.justification,
            ..Default::default()
        };
        let c = client(ctx)?.justify(req).await.map_err(grpc)?.into_inner().challenge;
        Ok(ChallengePayload { challenge: Challenge(c.ok_or("awzy-challenge returned no challenge")?) })
    }

    /// Approves or rejects someone else's request. awzy-challenge checks with the PDP that the
    /// signed-in person, or a service account such as an overseer agent (ADR 0065), may approve the
    /// resource.
    async fn decide_challenge(&self, ctx: &Context<'_>, input: DecideChallengeInput) -> Result<ChallengePayload> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        if viewer.on_behalf_of.is_some() {
            return Err(format!(
                "{} can't approve for someone else; it approves on its own authority or not at all",
                viewer.actor().label()
            )
            .into());
        }
        let req = DecideRequest {
            challenge_qid: input.challenge_qid.0,
            decided_by_actor: Some(viewer.actor().into()),
            approve: input.approve,
            note: input.note.unwrap_or_default(),
            approver_entities: viewer.entities.iter().map(awzy_authz_proto::entity_from_json).collect(),
            ..Default::default()
        };
        let c = client(ctx)?.decide(req).await.map_err(grpc)?.into_inner().challenge;
        Ok(ChallengePayload { challenge: Challenge(c.ok_or("awzy-challenge returned no challenge")?) })
    }

    /// Revokes a principal's access until it's lifted. Every PDP applies it within seconds.
    async fn contain_principal(&self, ctx: &Context<'_>, input: ContainPrincipalInput) -> Result<ContainmentPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let req = ContainRequest {
            principal: Some(awzy_authz_proto::authz::v1::EntityRef {
                r#type: input.principal.kind,
                id: input.principal.id,
            }),
            reason: input.reason,
            created_by_actor: Some(viewer.actor().into()),
            ..Default::default()
        };
        let c = client(ctx)?.contain(req).await.map_err(grpc)?.into_inner().containment;
        Ok(ContainmentPayload { containment: Containment(c.ok_or("awzy-challenge returned no containment")?) })
    }

    async fn lift_containment(&self, ctx: &Context<'_>, input: LiftContainmentInput) -> Result<ContainmentPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let req = LiftRequest {
            containment_qid: input.containment_qid.0,
            lifted_by_actor: Some(viewer.actor().into()),
            reason: input.reason,
            ..Default::default()
        };
        let c = client(ctx)?.lift(req).await.map_err(grpc)?.into_inner().containment;
        Ok(ContainmentPayload { containment: Containment(c.ok_or("awzy-challenge returned no containment")?) })
    }
}

/// Against a real awzy-challenge on Postgres, which asks the in-process PDP who may approve.
///   bazel test --config=local-stack //portals/client/backend:backend_test
/// Without AWZY_TEST_POSTGRES_URL they pass without checking anything.
#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc};

    use awzy_authz_proto::{authz::v1::ChallengeKind as WireKind, challenge::v1::ResolveRequest};
    use awzy_challenge::{approvals::Approvals, ChallengeStore, Feed, Lifetimes};
    use serde_json::json;
    use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
    use tonic::transport::Channel;

    use super::*;
    use crate::testing::{human, pdp, pdp_endpoint, run, run_err};

    /// The schema with a PDP and an awzy-challenge in a fresh Postgres schema.
    async fn setup() -> Option<(crate::ClientPortalSchema, ChallengeClient)> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&PgPool::connect(&url).await.unwrap()).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        let store = Arc::new(ChallengeStore::new(pool, Lifetimes::default()));
        store.migrate().await.unwrap();
        let keys = Arc::new(awzy_challenge::keys::SecurityKeys::new(
            store.pool().clone(),
            awzy_challenge::webauthn::RelyingParty::new("portal.example.com", ["https://portal.example.com"]),
            store.clone(),
        ));
        let app = awzy_challenge::router(
            store,
            Arc::new(Feed::default()),
            Arc::new(Approvals::pdp(&pdp_endpoint().await).unwrap()),
            Some(keys),
            &awzy_challenge::Access::off(),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let channel = Channel::from_shared(format!("http://{addr}")).unwrap().connect().await.unwrap();
        let challenge = ChallengeClient::new(awzy_service_auth::grpc_channel(channel, "awzy-challenge"));
        let services = Services { pdp: Some(pdp().await), challenge: Some(challenge.clone()), ..Default::default() };
        Some((crate::schema_with(services), challenge))
    }

    /// Opens a challenge for `who` reading the board deck, as a PDP would.
    async fn open(client: &mut ChallengeClient, who: &str, kind: WireKind) -> String {
        let req = ResolveRequest {
            decision_qid: awzy_model::DecisionId::new().to_string(),
            principal: Some(awzy_authz_proto::authz::v1::EntityRef { r#type: "Human".into(), id: who.into() }),
            on_behalf_of: None,
            action: "read".into(),
            resource: Some(awzy_authz_proto::authz::v1::EntityRef {
                r#type: "Resource".into(),
                id: "board-deck".into(),
            }),
            kinds: vec![kind.into()],
            answerer_id: who.into(),
            open: true,
            ..Default::default()
        };
        client.resolve(req).await.unwrap().into_inner().challenge_qid
    }

    #[tokio::test]
    async fn people_see_and_answer_only_their_own_challenges() {
        let Some((schema, mut client)) = setup().await else { return };
        let id = open(&mut client, "bob", WireKind::Justification).await;
        let (bob, carol, alice) =
            (human("bob", "engineering"), human("carol", "engineering"), human("alice", "security"));

        let mine = run(&schema, &bob, "{ myChallenges(status: PENDING) { id kinds status } }").await;
        assert_eq!(mine["myChallenges"], json!([{ "id": id, "kinds": ["JUSTIFICATION"], "status": "PENDING" }]));
        let theirs = run(&schema, &carol, "{ myChallenges { id } }").await;
        assert_eq!(theirs["myChallenges"], json!([]));
        let node = format!(r#"{{ node(id: "{id}") {{ id }} }}"#);
        assert!(run_err(&schema, &carol, node.as_str()).await.contains("may not administer challenges"));
        assert_eq!(run(&schema, &alice, node.as_str()).await["node"]["id"], json!(id), "an admin sees any");

        let justify = |text: &str| {
            format!(
                r#"mutation {{ justifyChallenge(input: {{ challengeQid: "{id}", justification: "{text}" }}) {{ challenge {{ status justification }} }} }}"#
            )
        };
        let why = "Preparing the audit committee's Q3 review";
        assert!(run_err(&schema, &carol, justify(why)).await.contains("only bob can answer"));
        assert!(run_err(&schema, &bob, justify("because")).await.contains("at least 20 characters"));
        let done = run(&schema, &bob, justify(why)).await;
        assert_eq!(done["justifyChallenge"]["challenge"], json!({ "status": "PASSED", "justification": why }));
    }

    #[tokio::test]
    async fn approvals_and_containments_are_for_those_policy_allows() {
        let Some((schema, mut client)) = setup().await else { return };
        let id = open(&mut client, "bob", WireKind::Approval).await;
        let (bob, carol, alice) =
            (human("bob", "engineering"), human("carol", "engineering"), human("alice", "security"));

        assert!(run_err(&schema, &bob, "{ challenges { id } }").await.contains("may not administer challenges"));
        // Each approver sees what policy lets them approve, and nobody their own request.
        let to_approve = |v: &Viewer| {
            let (schema, v) = (schema.clone(), v.clone());
            async move { run(&schema, &v, "{ challengesToApprove { id } }").await["challengesToApprove"].clone() }
        };
        assert_eq!(to_approve(&alice).await, json!([{ "id": id }]));
        assert_eq!(to_approve(&bob).await, json!([]), "bob asked");
        assert_eq!(to_approve(&carol).await, json!([]), "policy doesn't let carol approve the board deck");
        let pending = run(&schema, &alice, "{ challenges(status: PENDING) { id principal { id } } }").await;
        assert_eq!(pending["challenges"], json!([{ "id": id, "principal": { "id": "bob" } }]));

        let decide = format!(
            r#"mutation {{ decideChallenge(input: {{ challengeQid: "{id}", approve: true, note: "ok" }}) {{ challenge {{ status decidedBy {{ id }} }} }} }}"#
        );
        assert!(run_err(&schema, &bob, decide.as_str()).await.contains("doesn't let bob approve"));
        assert!(run_err(&schema, &carol, decide.as_str()).await.contains("doesn't let carol approve"));
        let approved = run(&schema, &alice, decide.as_str()).await;
        assert_eq!(
            approved["decideChallenge"]["challenge"],
            json!({ "status": "PASSED", "decidedBy": { "id": "alice" } })
        );

        let contain = r#"mutation { containPrincipal(input: { principal: { type: "ServiceAccount", id: "sales-genie" },
            reason: "bulk export of the pipeline" }) { containment { id createdBy { type id } liftedAt } } }"#;
        assert!(run_err(&schema, &bob, contain).await.contains("may not administer challenges"));
        let c = run(&schema, &alice, contain).await["containPrincipal"]["containment"].clone();
        assert_eq!((&c["createdBy"], &c["liftedAt"]), (&json!({ "type": "Human", "id": "alice" }), &json!(null)));
        let active = run(&schema, &alice, "{ containments { id principal { type id } } }").await;
        assert_eq!(
            active["containments"],
            json!([{ "id": c["id"], "principal": { "type": "ServiceAccount", "id": "sales-genie" } }])
        );

        let lift = format!(
            r#"mutation {{ liftContainment(input: {{ containmentQid: {}, reason: "reviewed" }}) {{ containment {{ liftedBy {{ id }} }} }} }}"#,
            c["id"]
        );
        assert_eq!(
            run(&schema, &alice, lift).await["liftContainment"]["containment"]["liftedBy"]["id"],
            json!("alice")
        );
        assert_eq!(run(&schema, &alice, "{ containments { id } }").await["containments"], json!([]));
    }

    #[tokio::test]
    async fn security_keys_start_with_the_portals_relying_party() {
        let Some((schema, mut client)) = setup().await else { return };
        let bob = human("bob", "engineering");
        assert_eq!(run(&schema, &bob, "{ mySecurityKeys { id } }").await["mySecurityKeys"], json!([]));

        let begin =
            run(&schema, &bob, "mutation { beginSecurityKeyRegistration(input: {}) { ceremonyQid optionsJson } }")
                .await;
        let c = &begin["beginSecurityKeyRegistration"];
        assert!(c["ceremonyQid"].as_str().unwrap().starts_with("ceremony_"));
        let options: serde_json::Value = serde_json::from_str(c["optionsJson"].as_str().unwrap()).unwrap();
        assert_eq!(options["rp"]["id"], "portal.example.com");
        assert_eq!(options["pubKeyCredParams"], json!([{ "type": "public-key", "alg": -7 }]));

        // Without a key, there's nothing to verify with, and nothing to authorize a change.
        let id = open(&mut client, "bob", WireKind::Verification).await;
        let verify = format!(
            r#"mutation {{ beginChallengeVerification(input: {{ challengeQid: "{id}" }}) {{ ceremonyQid }} }}"#
        );
        assert!(run_err(&schema, &bob, verify.as_str()).await.contains("has no security key"));
        let change = "mutation { beginSecurityKeyChange { ceremonyQid } }";
        assert!(run_err(&schema, &bob, change).await.contains("has no security key"));
        // Someone else can't start a verification of bob's challenge.
        let carol = human("carol", "engineering");
        assert!(run_err(&schema, &carol, verify.as_str()).await.contains("only bob can answer"));
    }

    #[tokio::test]
    async fn a_subscription_ends_when_the_token_it_started_with_does() {
        let mut bob = awzy_identity::Principal::human("bob", None, &[]);
        bob.expires_at = Some(SystemTime::now() - Duration::from_secs(1));
        let forever = async_graphql::futures_util::stream::pending::<()>();
        let stream = while_allowed(Services::default(), Viewer(Arc::new(bob), None), PortalAction::View, forever);
        let sent = tokio::time::timeout(Duration::from_secs(5), stream.collect::<Vec<_>>()).await;
        assert_eq!(sent.expect("ended at once"), Vec::<()>::new());
    }

    #[test]
    fn only_a_person_answers() {
        let mut agent = awzy_identity::Principal::human("bob", None, &[]);
        agent.subject = awzy_model::EntityRef::new("ServiceAccount", "sales-genie");
        agent.on_behalf_of = Some(awzy_model::EntityRef::new("Human", "bob"));
        let err = answering_human(&Viewer(Arc::new(agent), None)).unwrap_err();
        assert!(err.message.contains("sales-genie for bob can't answer challenges"), "{}", err.message);
        assert_eq!(answering_human(&crate::testing::human("bob", "engineering")).unwrap(), "bob");
    }
}

/// Live updates for the Challenges page, from awzy-challenge's streams.
#[derive(Default)]
pub struct ChallengeSubscription;

#[Subscription]
impl ChallengeSubscription {
    /// Challenges as they're opened or change: the signed-in person's own, or every one for those
    /// who administer challenges.
    async fn challenge_updated(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = Challenge>> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let services = ctx.data::<Services>()?;
        let every = authorize(services, &viewer, Surface::Portal, PortalAction::Administer, PortalArea::Challenges)
            .await
            .is_ok();
        let answerer_id = if every { String::new() } else { answering_human(&viewer)? };
        let stream = client(ctx)?.watch_challenges(WatchChallengesRequest { answerer_id }).await.map_err(grpc)?;
        let stream = StreamExt::filter_map(stream.into_inner(), |m| async move {
            // A heartbeat carries no challenge; a broken stream ends the subscription.
            m.ok()?.challenge.map(Challenge)
        });
        let action = if every { PortalAction::Administer } else { PortalAction::View };
        Ok(while_allowed(services.clone(), viewer, action, stream))
    }

    /// Every active containment, whenever one is added or lifted. For those who administer
    /// challenges.
    async fn containments_changed(&self, ctx: &Context<'_>) -> Result<impl Stream<Item = Vec<Containment>>> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let stream = client(ctx)?.watch_containments(WatchContainmentsRequest {}).await.map_err(grpc)?;
        let stream = StreamExt::filter_map(stream.into_inner(), |m| async move {
            let m = m.ok()?;
            m.changed.then(|| m.active.into_iter().map(Containment).collect())
        });
        Ok(while_allowed(ctx.data::<Services>()?.clone(), viewer, PortalAction::Administer, stream))
    }
}

/// How often a live subscription checks that its viewer may still see what it sends.
const RECHECK: Duration = Duration::from_secs(60);

/// `stream` until the viewer's token expires or they may no longer do `action` on the challenges
/// area: a subscription is authorized when it starts, and would otherwise outlive both. If the PDP
/// can't be asked, it ends too; the page subscribes again.
fn while_allowed<S: Stream + Send + 'static>(
    services: Services,
    viewer: Viewer,
    action: PortalAction,
    stream: S,
) -> impl Stream<Item = S::Item> {
    let ended = async move {
        let expired = async {
            match viewer.expires_at {
                Some(at) => tokio::time::sleep(at.duration_since(SystemTime::now()).unwrap_or_default()).await,
                None => std::future::pending().await,
            }
        };
        let revoked = async {
            loop {
                tokio::time::sleep(RECHECK).await;
                if authorize(&services, &viewer, Surface::Portal, action, PortalArea::Challenges).await.is_err() {
                    return;
                }
            }
        };
        tokio::select! {
            _ = expired => {}
            _ = revoked => {}
        }
    };
    StreamExt::take_until(stream, Box::pin(ended))
}
