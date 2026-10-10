//! Challenges and containments (ADR 0054), through awzy-challenge.
//!
//! - Anyone who may `view` the challenges area answers the challenges sent to them, and may
//!   approve someone else's request where policy lets them (awzy-challenge asks the PDP).
//! - Those who may `administer` it see every challenge and contain or lift accessors.
//!
//! Only a person answers or approves: a service account signed in, or acting for someone, is
//! refused, so an agent can never pass its own challenge.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject, ID};
use awzy_authz_proto::{
    challenge::v1::{
        challenge_service_client::ChallengeServiceClient, Challenge as WireChallenge, ChallengeStatus as WireStatus,
        ContainRequest, Containment as WireContainment, DecideRequest, GetChallengeRequest, JustifyRequest,
        LiftRequest, ListChallengesRequest, ListContainmentsRequest,
    },
    challenges_from_wire,
    prost_types::Timestamp,
};
use awzy_model::{entity_type, ChallengeId};
use awzy_portal::{Actor, UtcTimestamp};
use chrono::DateTime;

use crate::{
    access::{require, PortalAction, PortalArea, Viewer},
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
pub(crate) fn answering_person(viewer: &Viewer) -> Result<String> {
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
    async fn decision_qid(&self) -> &str {
        &self.0.decision_qid
    }

    async fn subject(&self) -> Option<EntityRef> {
        entity(&self.0.subject)
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

/// An accessor whose access is revoked until a person lifts it.
pub struct Containment(pub WireContainment);

#[Object]
impl Containment {
    /// QID: containment_<uuid>.
    async fn id(&self) -> ID {
        ID(self.0.qid.clone())
    }

    async fn subject(&self) -> Option<EntityRef> {
        entity(&self.0.subject)
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
    /// Recorded but not applied: awzy-reasoner would have contained this accessor, and automated
    /// containments are in monitor (ADR 0066). Lifting it dismisses it.
    async fn monitor(&self) -> bool {
        self.0.monitor
    }
}

/// For `node(id)`: the challenge, if the viewer is the person it was sent to or administers
/// challenges.
pub(crate) async fn get_by_id(ctx: &Context<'_>, id: ChallengeId) -> Result<Option<Challenge>> {
    let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
    let res = client(ctx)?.get_challenge(GetChallengeRequest { challenge_qid: id.to_string() }).await;
    let c = match res {
        Ok(r) => r.into_inner().challenge,
        Err(s) if s.code() == tonic::Code::NotFound => None,
        Err(s) => return Err(grpc(s)),
    };
    let Some(c) = c else { return Ok(None) };
    if c.answerer_id != viewer.subject.id || viewer.subject.kind != entity_type::HUMAN {
        require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
    }
    Ok(Some(Challenge(c)))
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
        let me = answering_person(&viewer)?;
        list(ctx, me, status, first).await
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
            ListContainmentsRequest { active_only, subject: None, limit: self::first(first), include_monitor: true };
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
pub struct ContainAccessorInput {
    pub subject: EntityRefInput,
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
            answered_by_actor: Some(awzy_model::Actor::human(answering_person(&viewer)?).into()),
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

    /// Revokes an accessor's access until it's lifted. Every PDP applies it within seconds.
    async fn contain_accessor(&self, ctx: &Context<'_>, input: ContainAccessorInput) -> Result<ContainmentPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let req = ContainRequest {
            subject: Some(awzy_authz_proto::authz::v1::EntityRef { r#type: input.subject.kind, id: input.subject.id }),
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
    use crate::testing::{pdp, pdp_endpoint, person, run, run_err};

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
            subject: Some(awzy_authz_proto::authz::v1::EntityRef { r#type: "Human".into(), id: who.into() }),
            on_behalf_of: None,
            action: "read".into(),
            resource: Some(awzy_authz_proto::authz::v1::EntityRef {
                r#type: "Resource".into(),
                id: "board-deck".into(),
            }),
            kinds: vec![kind.into()],
            answerer_id: who.into(),
            open: true,
        };
        client.resolve(req).await.unwrap().into_inner().challenge_qid
    }

    #[tokio::test]
    async fn people_see_and_answer_only_their_own_challenges() {
        let Some((schema, mut client)) = setup().await else { return };
        let id = open(&mut client, "bob", WireKind::Justification).await;
        let (bob, carol, alice) =
            (person("bob", "engineering"), person("carol", "engineering"), person("alice", "security"));

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
            (person("bob", "engineering"), person("carol", "engineering"), person("alice", "security"));

        assert!(run_err(&schema, &bob, "{ challenges { id } }").await.contains("may not administer challenges"));
        let pending = run(&schema, &alice, "{ challenges(status: PENDING) { id subject { id } } }").await;
        assert_eq!(pending["challenges"], json!([{ "id": id, "subject": { "id": "bob" } }]));

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

        let contain = r#"mutation { containAccessor(input: { subject: { type: "ServiceAccount", id: "sales-genie" },
            reason: "bulk export of the pipeline" }) { containment { id createdBy { type id } liftedAt } } }"#;
        assert!(run_err(&schema, &bob, contain).await.contains("may not administer challenges"));
        let c = run(&schema, &alice, contain).await["containAccessor"]["containment"].clone();
        assert_eq!((&c["createdBy"], &c["liftedAt"]), (&json!({ "type": "Human", "id": "alice" }), &json!(null)));
        let active = run(&schema, &alice, "{ containments { id subject { type id } } }").await;
        assert_eq!(
            active["containments"],
            json!([{ "id": c["id"], "subject": { "type": "ServiceAccount", "id": "sales-genie" } }])
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
        let bob = person("bob", "engineering");
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
        let carol = person("carol", "engineering");
        assert!(run_err(&schema, &carol, verify.as_str()).await.contains("only bob can answer"));
    }

    #[test]
    fn only_a_person_answers() {
        let mut agent = awzy_identity::Accessor::human("bob", None, &[]);
        agent.subject = awzy_model::EntityRef::new("ServiceAccount", "sales-genie");
        agent.on_behalf_of = Some(awzy_model::EntityRef::new("Human", "bob"));
        let err = answering_person(&Viewer(Arc::new(agent))).unwrap_err();
        assert!(err.message.contains("sales-genie for bob can't answer challenges"), "{}", err.message);
        assert_eq!(answering_person(&crate::testing::person("bob", "engineering")).unwrap(), "bob");
    }
}
