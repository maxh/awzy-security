//! awzy-pdp: the policy decision point on the hot path.
//!
//! Evaluates one action on one resource against Cedar policies and returns allow, deny, or
//! challenge. A `forbid` policy annotated `@challenge("<kind>")` turns a deny into a challenge.
//! It never calls a model or ClickHouse: everything it needs arrives as entity attributes, from
//! the enforcement point or, with awzy-context, from its principals snapshot ([`principals`]) and
//! the resource lookup ([`resources`]).

mod decision_log;
mod enforcement;
mod grpc;
pub mod metrics;
pub mod principals;
pub mod replay;
pub mod resources;
mod sync;

pub use decision_log::{DecisionLog, DEFAULT_DECISIONS_TOPIC};
pub use enforcement::Enforcement;
pub use sync::{sync_once, sync_policies};

use std::{collections::HashSet, str::FromStr, sync::Arc};

use arc_swap::ArcSwap;

use awzy_model::{
    AccessRequest, ChallengeKind, Decision, DecisionId, DecisionRecord, DelegationCheck, EnforcementMode, EntityRef,
    Verdict,
};
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use cedar_policy::{
    Authorizer, Context, Entities, EntityId, EntityTypeName, EntityUid, PolicySet, Request, Schema, ValidationMode,
    Validator,
};
use serde::Deserialize;

/// Base schema and policies shipped with awzy; clients layer their own bundles on top.
pub const DEFAULT_SCHEMA: &str = include_str!("../../../../policies/awzy.cedarschema");
pub const DEFAULT_POLICIES: &str = include_str!("../../../../policies/base.cedar");

pub struct Pdp {
    schema: Schema,
    policies: PolicySet,
    authorizer: Authorizer,
    /// Version of the policy bundle in force; 0 for the built-in defaults.
    version: i64,
    /// Whether the bundle's schema declares `Resource.unclassified`. A bundle published before
    /// it existed doesn't, and Cedar rejects attributes a schema doesn't declare.
    knows_unclassified: bool,
    /// Whether the schema declares `context.assigned` for `read` (ADR 0046). Same reason.
    knows_assigned: bool,
}

/// The PDP every request reads. Policy sync swaps in a new one without blocking readers.
pub type SharedPdp = Arc<ArcSwap<Pdp>>;

pub fn shared(pdp: Pdp) -> SharedPdp {
    Arc::new(ArcSwap::from_pointee(pdp))
}

/// The source text of the policies in `policies` whose ID (the `@id` annotation, else Cedar's
/// generated ID) is in `ids`. Used to explain a decision from the bundle it was made under.
pub fn policy_sources(policies: &str, ids: &[String]) -> anyhow::Result<Vec<(String, String)>> {
    let set = PolicySet::from_str(policies)?;
    Ok(set
        .policies()
        .filter_map(|p| {
            let id = p.annotation("id").map(str::to_string).unwrap_or_else(|| p.id().to_string());
            ids.contains(&id).then(|| (id, p.to_string()))
        })
        .collect())
}

/// Parses `policies` and validates them strictly against `schema`. awzy-policy calls this
/// before storing a draft, so a bundle that reaches a PDP always loads.
pub fn validate(schema: &str, policies: &str) -> anyhow::Result<()> {
    Pdp::new(schema, policies).map(|_| ())
}

impl Pdp {
    /// Parses and strictly validates `policies` against `schema`.
    pub fn new(schema: &str, policies: &str) -> anyhow::Result<Self> {
        let (schema, _warnings) = Schema::from_cedarschema_str(schema)?;
        let policies = PolicySet::from_str(policies)?;
        let result = Validator::new(schema.clone()).validate(&policies, ValidationMode::Strict);
        if !result.validation_passed() {
            let errors: Vec<String> = result.validation_errors().map(|e| e.to_string()).collect();
            anyhow::bail!("policy validation failed: {}", errors.join("; "));
        }
        let probe = serde_json::json!([{ "uid": { "type": "Resource", "id": "probe" },
            "attrs": { "sensitivity": "internal", "unclassified": true }, "parents": [] }]);
        let knows_unclassified = Entities::from_json_value(probe, Some(&schema)).is_ok();
        let read = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new("read"));
        let probe = serde_json::json!({ "risk_score": 0, "human_present": true, "assigned": true });
        let knows_assigned = Context::from_json_value(probe, Some((&schema, &read))).is_ok();
        Ok(Self { schema, policies, authorizer: Authorizer::new(), version: 0, knows_unclassified, knows_assigned })
    }

    pub fn with_version(mut self, version: i64) -> Self {
        self.version = version;
        self
    }

    pub fn version(&self) -> i64 {
        self.version
    }

    pub fn with_defaults() -> anyhow::Result<Self> {
        Self::new(DEFAULT_SCHEMA, DEFAULT_POLICIES)
    }

    /// Parses Cedar entity JSON (accessors, teams, resources) against the schema.
    pub fn entities(&self, mut json: serde_json::Value) -> anyhow::Result<Entities> {
        if !self.knows_unclassified {
            for e in json.as_array_mut().into_iter().flatten() {
                if e["uid"]["type"] == awzy_model::entity_type::RESOURCE {
                    if let Some(attrs) = e["attrs"].as_object_mut() {
                        attrs.remove("unclassified");
                    }
                }
            }
        }
        Ok(Entities::from_json_value(json, Some(&self.schema))?)
    }

    /// Decides one request. A delegated request (`on_behalf_of` set) is checked twice: once
    /// for the subject and once as the person it acts for. The stricter verdict wins, so a
    /// service account can never do more than the person it acts for.
    pub fn decide(&self, req: &AccessRequest, entities: &Entities) -> anyhow::Result<Decision> {
        self.decide_assigned(req, entities, None)
    }

    /// [`Self::decide`], with who is assigned to work on the resource when that's known
    /// (ADR 0046). Each check then sees `context.assigned` for its own principal: the account for
    /// the subject's check, the person for the delegated one.
    pub fn decide_assigned(
        &self,
        req: &AccessRequest,
        entities: &Entities,
        assignees: Option<&HashSet<String>>,
    ) -> anyhow::Result<Decision> {
        for key in ["on_behalf_of", "human_present", "assigned"] {
            anyhow::ensure!(
                !req.context.contains_key(key),
                "context key {key:?} is set by the PDP; use the request field"
            );
        }
        let mut context = req.context.clone();
        // TODO: from awzy-reasoner's risk attributes. Enforcement points can't know risk.
        context.entry("risk_score").or_insert(serde_json::Value::from(0));
        context.insert("human_present".into(), serde_json::Value::Bool(req.human_present()));
        if let Some(h) = &req.on_behalf_of {
            context.insert("on_behalf_of".into(), serde_json::json!({ "__entity": { "type": h.kind, "id": h.id } }));
        }

        let assignees = assignees.filter(|_| self.knows_assigned);
        let (verdict, policies) = self.check(&req.subject, req, &context, entities, assignees)?;
        let on_behalf_of = match &req.on_behalf_of {
            Some(human) => {
                let (verdict, policies) = self.check(human, req, &context, entities, assignees)?;
                Some(DelegationCheck { verdict, policies })
            }
            None => None,
        };
        let verdict = on_behalf_of.as_ref().map_or(verdict, |d| verdict.stricter(d.verdict));
        // AppState::check stamps the install's mode; a bare decision is never enforced by default.
        Ok(Decision { id: DecisionId::new(), verdict, policies, on_behalf_of, mode: EnforcementMode::Monitor })
    }

    /// One Cedar evaluation with `principal`. Returns the verdict and determining policy IDs.
    fn check(
        &self,
        principal: &EntityRef,
        req: &AccessRequest,
        context: &serde_json::Map<String, serde_json::Value>,
        entities: &Entities,
        assignees: Option<&HashSet<String>>,
    ) -> anyhow::Result<(Verdict, Vec<String>)> {
        let action = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new(&req.action));
        let mut context = context.clone();
        if let Some(assignees) = assignees {
            context.insert("assigned".into(), serde_json::Value::Bool(assignees.contains(&principal.id)));
        }
        let context = Context::from_json_value(serde_json::Value::Object(context), Some((&self.schema, &action)))?;
        let request = Request::new(uid(principal)?, action, uid(&req.resource)?, context, Some(&self.schema))?;
        let response = self.authorizer.is_authorized(&request, &self.policies, entities);

        let determining: Vec<_> = response.diagnostics().reason().filter_map(|id| self.policies.policy(id)).collect();
        // Cedar reports them in no particular order; sort so explanations are stable.
        let mut policies: Vec<String> = determining
            .iter()
            .map(|p| p.annotation("id").map(str::to_string).unwrap_or_else(|| p.id().to_string()))
            .collect();
        policies.sort();
        let verdict = match response.decision() {
            cedar_policy::Decision::Allow => Verdict::Allow,
            cedar_policy::Decision::Deny => {
                // Challenge only when every forbid that fired asks for one; then all of them are required.
                let kinds: Option<Vec<ChallengeKind>> =
                    determining.iter().map(|p| p.annotation("challenge").and_then(ChallengeKind::parse)).collect();
                match kinds {
                    Some(kinds) if !kinds.is_empty() => Verdict::Challenge { challenges: kinds.into_iter().collect() },
                    _ => Verdict::Deny,
                }
            }
        };
        Ok((verdict, policies))
    }
}

fn uid(e: &EntityRef) -> anyhow::Result<EntityUid> {
    Ok(EntityUid::from_type_name_and_id(EntityTypeName::from_str(&e.kind)?, EntityId::new(&e.id)))
}

/// What every request handler shares: the PDP in force, the enforcement mode, and, when
/// configured, the decision log.
#[derive(Clone)]
pub struct AppState {
    pub pdp: SharedPdp,
    pub enforcement: Arc<Enforcement>,
    pub log: Option<Arc<DecisionLog>>,
    pub metrics: Arc<metrics::Metrics>,
    /// People and groups from awzy-context, if configured.
    pub principals: Arc<principals::Principals>,
    /// Resources from awzy-context, if configured.
    pub resources: Arc<resources::Resources>,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    /// The PDP hasn't synced its enforcement mode yet. Enforcement points use their fallback.
    #[error("the PDP is starting and hasn't loaded its enforcement mode or principals yet")]
    NotReady,
    /// awzy-context can't be reached to load the resource, and it isn't cached. Enforcement
    /// points use their fallback, as when the PDP itself is unreachable.
    #[error("can't load the resource from awzy-context: {0}")]
    Unavailable(anyhow::Error),
    #[error(transparent)]
    Invalid(#[from] anyhow::Error),
}

impl AppState {
    /// In monitor mode, ready at once. For tests and a PDP with no awzy-policy.
    pub fn new(pdp: SharedPdp) -> Self {
        Self {
            pdp,
            enforcement: Arc::new(Enforcement::fixed(EnforcementMode::Monitor)),
            log: None,
            metrics: Arc::default(),
            principals: Arc::new(principals::Principals::none()),
            resources: Arc::new(resources::Resources::none()),
        }
    }

    pub fn with_resources(mut self, resources: Arc<resources::Resources>) -> Self {
        self.resources = resources;
        self
    }

    pub fn with_enforcement(mut self, enforcement: Arc<Enforcement>) -> Self {
        self.enforcement = enforcement;
        self
    }

    pub fn with_principals(mut self, principals: Arc<principals::Principals>) -> Self {
        self.principals = principals;
        self
    }

    /// Ready once the policy bundle and mode are loaded and, with awzy-context, the first
    /// principals snapshot.
    pub fn is_ready(&self) -> bool {
        self.enforcement.is_ready() && self.principals.is_ready()
    }

    pub fn with_log(mut self, log: Arc<DecisionLog>) -> Self {
        self.log = Some(log);
        self
    }

    /// Decides one request, stamps the enforcement mode, and records the decision. Recording
    /// never blocks or fails a decision.
    pub async fn check(&self, req: &AccessRequest, entities: serde_json::Value) -> Result<Decision, CheckError> {
        let started = std::time::Instant::now();
        let result = self.check_inner(req, entities).await;
        self.metrics.record(result.as_ref().ok().map(|d| &d.verdict), started.elapsed());
        result
    }

    async fn check_inner(&self, req: &AccessRequest, entities: serde_json::Value) -> Result<Decision, CheckError> {
        if !self.is_ready() {
            return Err(CheckError::NotReady);
        }
        // With awzy-context, every service account must be registered there (ADR 0033).
        let assembled = replay::assemble(&self.principals, &self.resources, req, entities)
            .await
            .map_err(CheckError::Unavailable)?;
        // Loaded after the lookup, which may wait on awzy-context, so a sync isn't held up.
        let pdp = self.pdp.load();
        let mut decision = match assembled {
            replay::Assembled::Unregistered => replay::unregistered(),
            replay::Assembled::Ready { entities, assignees } => {
                pdp.decide_assigned(req, &pdp.entities(entities)?, assignees.as_deref())?
            }
        };
        decision.mode = if req.always_enforced { EnforcementMode::Enforce } else { self.enforcement.mode() };
        if let Some(log) = &self.log {
            log.record(DecisionRecord::new(req, &decision, pdp.version(), chrono::Utc::now()));
        }
        Ok(decision)
    }
}

/// AuthZEN-style evaluation request. Entities are inline until the attribute snapshot lands.
#[derive(Debug, Deserialize)]
pub struct EvaluationRequest {
    #[serde(flatten)]
    pub request: AccessRequest,
    #[serde(default)]
    pub entities: Vec<serde_json::Value>,
}

/// HTTP (AuthZEN-style) and gRPC on one port. axum serves HTTP/1 and HTTP/2 cleartext,
/// so gRPC clients connect to the same address.
pub fn router(state: AppState) -> Router {
    let metrics = state.metrics.clone();
    Router::new()
        .route("/v1/access/evaluation", post(evaluate))
        .route(
            "/metrics",
            axum::routing::get(move || {
                let metrics = metrics.clone();
                async move { ([(axum::http::header::CONTENT_TYPE, "text/plain; version=0.0.4")], metrics.render()) }
            }),
        )
        .with_state(state.clone())
        .merge(grpc::router(state))
}

async fn evaluate(
    State(state): State<AppState>,
    Json(body): Json<EvaluationRequest>,
) -> Result<Json<Decision>, (StatusCode, String)> {
    state.check(&body.request, serde_json::Value::Array(body.entities)).await.map(Json).map_err(|e| match e {
        CheckError::NotReady | CheckError::Unavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
        CheckError::Invalid(_) => (StatusCode::BAD_REQUEST, e.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_model::Challenges;
    use serde_json::json;

    /// alice is on security, bob on engineering. Two AI service accounts: sales-genie
    /// (engineering, owned by bob) and ops-bot (security, owned by alice).
    fn fixtures(pdp: &Pdp) -> Entities {
        pdp.entities(json!([
            { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
              "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true },
              "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "ServiceAccount", "id": "ops-bot" },
              "attrs": { "owner": { "__entity": { "type": "Human", "id": "alice" } }, "ai": true },
              "parents": [{ "type": "Group", "id": "security" }] },
            { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
            { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
        ]))
        .unwrap()
    }

    fn read(who: &str, what: &str, risk: i64) -> AccessRequest {
        AccessRequest {
            subject: EntityRef::new("Human", who),
            action: "read".into(),
            resource: EntityRef::new("Resource", what),
            context: json!({ "risk_score": risk }).as_object().unwrap().clone(),
            on_behalf_of: None,
            human_present: None,
            always_enforced: false,
        }
    }

    /// `account` reading `what`, optionally acting for the person `for_human`.
    fn sa_read(account: &str, for_human: Option<&str>, what: &str, risk: i64) -> AccessRequest {
        AccessRequest {
            subject: EntityRef::new("ServiceAccount", account),
            on_behalf_of: for_human.map(|h| EntityRef::new("Human", h)),
            ..read(account, what, risk)
        }
    }

    fn decide(req: &AccessRequest) -> Decision {
        let pdp = Pdp::with_defaults().unwrap();
        pdp.decide(req, &fixtures(&pdp)).unwrap()
    }

    #[test]
    fn a_bundle_published_before_unclassified_still_loads_resources() {
        let old_schema = DEFAULT_SCHEMA.replace("unclassified?: Bool,", "");
        assert_ne!(old_schema, DEFAULT_SCHEMA);
        let pdp = Pdp::new(&old_schema, DEFAULT_POLICIES).unwrap();
        let resource = json!([{ "uid": { "type": "Resource", "id": "doc" },
            "attrs": { "sensitivity": "internal", "unclassified": true }, "parents": [] }]);
        assert!(pdp.entities(resource.clone()).is_ok(), "the attribute is dropped, not an error");
        assert!(Pdp::with_defaults().unwrap().entities(resource).is_ok());
    }

    #[test]
    fn assignment_is_checked_for_the_agent_and_the_person_it_acts_for() {
        let policies = r#"
            @id("anyone-reads")
            permit (principal, action == Action::"read", resource);
            @id("crown-jewels-need-assigned-work")
            @challenge("approval")
            forbid (principal, action == Action::"read", resource)
            when { resource.sensitivity == "crown_jewels" && context has assigned && !context.assigned };
        "#;
        let pdp = Pdp::new(DEFAULT_SCHEMA, policies).unwrap();
        let entities = pdp
            .entities(json!([
                { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [] },
                { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                  "attrs": { "owner": { "__entity": { "type": "Human", "id": "nora" } }, "ai": true }, "parents": [] },
                { "uid": { "type": "Resource", "id": "plan" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
            ]))
            .unwrap();
        let req = AccessRequest { human_present: Some(true), ..sa_read("sales-genie", Some("nora"), "plan", 1) };
        let assigned = |who: &[&str]| who.iter().map(|w| w.to_string()).collect::<HashSet<String>>();
        let challenged = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) };

        // The rogue agent: neither SalesGenie nor Nora has work that needs the plan.
        let d = pdp.decide_assigned(&req, &entities, Some(&assigned(&["strategy-lead"]))).unwrap();
        assert_eq!((d.verdict, d.on_behalf_of.unwrap().verdict), (challenged, challenged));

        // The agent alone being assigned isn't enough: the person's check still needs it.
        let d = pdp.decide_assigned(&req, &entities, Some(&assigned(&["sales-genie"]))).unwrap();
        assert_eq!(d.policies, vec!["anyone-reads"], "the account's own check passed");
        assert_eq!(d.verdict, challenged);

        let both = assigned(&["sales-genie", "nora"]);
        assert_eq!(pdp.decide_assigned(&req, &entities, Some(&both)).unwrap().verdict, Verdict::Allow);
        // Unknown (no awzy-context): the policy can't tell, and doesn't fire.
        assert_eq!(pdp.decide(&req, &entities).unwrap().verdict, Verdict::Allow);

        // A bundle published before `assigned` existed still decides.
        let old = Pdp::new(&DEFAULT_SCHEMA.replace("assigned?: Bool,", ""), DEFAULT_POLICIES).unwrap();
        let entities = old.entities(json!([{ "uid": { "type": "Resource", "id": "plan" }, "attrs": { "sensitivity": "internal" }, "parents": [] }])).unwrap();
        assert!(old.decide_assigned(&read("bob", "plan", 1), &entities, Some(&both)).is_ok());
    }

    #[test]
    fn policy_sources_by_id() {
        let found = policy_sources(DEFAULT_POLICIES, &["high-risk-verify".into()]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "high-risk-verify");
        assert!(found[0].1.contains("context.risk_score > 80"), "{}", found[0].1);
    }

    #[test]
    fn security_team_reads_anything() {
        let d = decide(&read("alice", "board-deck", 10));
        assert_eq!(d.verdict, Verdict::Allow);
        assert_eq!(d.policies, vec!["security-team-read"]);
        assert!(d.on_behalf_of.is_none());
    }

    #[test]
    fn engineering_cannot_read_crown_jewels() {
        assert_eq!(decide(&read("bob", "board-deck", 10)).verdict, Verdict::Deny);
    }

    #[test]
    fn high_risk_turns_allow_into_challenge() {
        let d = decide(&read("bob", "design-doc", 90));
        assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) });
        assert_eq!(d.policies, vec!["high-risk-verify"]);
    }

    #[test]
    fn delegation_never_exceeds_the_person() {
        // ops-bot (security) could read the board deck itself; bob can't, so acting for bob it can't.
        let d = decide(&sa_read("ops-bot", Some("bob"), "board-deck", 10));
        assert_eq!(d.verdict, Verdict::Deny);
        assert_eq!(d.policies, vec!["security-team-read"], "the account's own check allowed");
        assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Deny);
    }

    #[test]
    fn delegation_never_exceeds_the_account() {
        // alice could read the board deck; sales-genie (engineering) can't, even for her.
        let d = decide(&sa_read("sales-genie", Some("alice"), "board-deck", 10));
        assert_eq!(d.verdict, Verdict::Deny);
        let human = d.on_behalf_of.unwrap();
        assert_eq!((human.verdict, human.policies), (Verdict::Allow, vec!["security-team-read".to_string()]));
    }

    #[test]
    fn unattended_ai_never_reads_crown_jewels() {
        let d = decide(&sa_read("ops-bot", None, "board-deck", 10));
        assert_eq!(d.verdict, Verdict::Deny);
        assert_eq!(d.policies, vec!["unattended-ai-no-crown-jewels"]);
    }

    #[test]
    fn absent_person_needs_approval_when_risky() {
        let mut req = sa_read("sales-genie", Some("bob"), "design-doc", 60);
        let d = decide(&req);
        assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) });
        assert_eq!(d.policies, vec!["absent-human-approval"]);
        assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Allow);

        req.human_present = Some(true);
        assert_eq!(decide(&req).verdict, Verdict::Allow, "a person present can answer for it");
    }

    #[test]
    fn every_challenge_that_fires_is_required() {
        // Risk 90 with no person present: absent-human-approval and high-risk-verify both fire.
        let d = decide(&sa_read("sales-genie", Some("bob"), "design-doc", 90));
        let all = [ChallengeKind::Verification, ChallengeKind::Approval].into_iter().collect();
        assert_eq!(d.verdict, Verdict::Challenge { challenges: all });
        assert_eq!(d.policies, vec!["absent-human-approval", "high-risk-verify"]);
    }

    #[test]
    fn callers_cannot_set_delegation_in_the_context() {
        let pdp = Pdp::with_defaults().unwrap();
        for key in ["on_behalf_of", "human_present", "assigned"] {
            let mut req = read("bob", "design-doc", 10);
            req.context.insert(key.into(), json!(true));
            let err = pdp.decide(&req, &fixtures(&pdp)).unwrap_err();
            assert!(err.to_string().contains("is set by the PDP"), "{err}");
        }
    }

    #[tokio::test]
    async fn the_idps_groups_and_deactivation_win_over_the_token() {
        let principals = Arc::new(principals::Principals::awaiting_sync());
        let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_principals(principals.clone());
        // What the enforcement points send, from tokens: bob in engineering, alice in security.
        let entities = json!([
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
            { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
        ]);
        assert!(
            matches!(state.check(&read("bob", "board-deck", 1), entities.clone()).await, Err(CheckError::NotReady)),
            "waits for awzy-context"
        );

        // The IdP has since moved bob into security and deactivated alice.
        principals.load(
            1,
            vec![
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "security" }] }),
                json!({ "uid": { "type": "Human", "id": "alice" }, "attrs": { "active": false }, "parents": [] }),
                json!({ "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] }),
            ],
        );
        let bob = state.check(&read("bob", "board-deck", 1), entities.clone()).await.unwrap();
        assert_eq!((bob.verdict, bob.policies.clone()), (Verdict::Allow, vec!["security-team-read".to_string()]));
        let alice = state.check(&read("alice", "board-deck", 1), entities).await.unwrap();
        assert_eq!((alice.verdict, alice.policies), (Verdict::Deny, vec!["deactivated-people".to_string()]));

        // A service account awzy-context doesn't know is denied, whatever the request says.
        let rogue = json!([{ "uid": { "type": "ServiceAccount", "id": "rogue-bot" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] }]);
        let d = state.check(&sa_read("rogue-bot", None, "board-deck", 1), rogue).await.unwrap();
        assert_eq!((d.verdict, d.policies), (Verdict::Deny, vec![principals::UNREGISTERED.to_string()]));
    }

    #[tokio::test]
    async fn check_stamps_the_mode_and_waits_for_the_first_sync() {
        let entities = json!([
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
        ]);
        let enforcement = Arc::new(Enforcement::awaiting_sync());
        let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_enforcement(enforcement.clone());
        assert!(matches!(
            state.check(&read("bob", "board-deck", 1), entities.clone()).await,
            Err(CheckError::NotReady)
        ));

        enforcement.synced(EnforcementMode::Monitor);
        let d = state.check(&read("bob", "board-deck", 1), entities.clone()).await.unwrap();
        assert_eq!((d.verdict, d.mode), (Verdict::Deny, EnforcementMode::Monitor), "a would-deny");
        let portal = AccessRequest { always_enforced: true, ..read("bob", "board-deck", 1) };
        assert_eq!(
            state.check(&portal, entities.clone()).await.unwrap().mode,
            EnforcementMode::Enforce,
            "applied whatever the mode"
        );

        enforcement.synced(EnforcementMode::Enforce);
        assert_eq!(state.check(&read("bob", "board-deck", 1), entities).await.unwrap().mode, EnforcementMode::Enforce);
    }

    #[tokio::test]
    async fn resources_are_decided_on_what_awzy_context_says() {
        use axum::{extract::Path, routing::get};

        // awzy-context knows the board deck is crown jewels; nothing else.
        let app = Router::new().route(
            "/v1/resources/{id}/entities",
            get(|Path(id): Path<String>| async move {
                if id != "board-deck" {
                    return Err(StatusCode::NOT_FOUND);
                }
                Ok(Json(json!({ "entities": [{ "uid": { "type": "Resource", "id": "board-deck" },
                    "attrs": { "sensitivity": "crown_jewels", "unclassified": false }, "parents": [] }] })))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let state = AppState::new(shared(Pdp::with_defaults().unwrap()))
            .with_resources(Arc::new(resources::Resources::from_context(url)));
        let engineer = json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] });
        let bob = || engineer.clone();

        // The enforcement point claims it's internal; awzy-context's tier decides.
        let claimed = json!([bob(), { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
        assert_eq!(state.check(&read("bob", "board-deck", 1), claimed).await.unwrap().verdict, Verdict::Deny);

        // Only an ID, for a resource awzy-context doesn't know: the default tier.
        let d = state.check(&read("bob", "not-synced-yet", 1), json!([bob()])).await.unwrap();
        assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["engineering-read-internal".to_string()]));

        let down = AppState::new(shared(Pdp::with_defaults().unwrap()))
            .with_resources(Arc::new(resources::Resources::from_context("http://127.0.0.1:1".into())));
        assert!(matches!(
            down.check(&read("bob", "board-deck", 1), json!([bob()])).await,
            Err(CheckError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn http_evaluation() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let body = json!({
            "subject": { "type": "Human", "id": "bob" },
            "action": "read",
            "resource": { "type": "Resource", "id": "design-doc" },
            "context": { "risk_score": 5 },
            "entities": [
                { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
                { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
            ],
        });
        let res = router(AppState::new(shared(Pdp::with_defaults().unwrap())))
            .oneshot(
                Request::post("/v1/access/evaluation")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let decision: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decision["verdict"], "allow");
        assert_eq!(decision["mode"], "monitor");
    }
}
