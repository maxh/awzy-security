//! awzy-pdp: the policy decision point on the hot path.
//!
//! Evaluates one action on one resource against Cedar policies and returns allow, deny, or
//! challenge. A `forbid` policy annotated `@challenge("<kind>")` turns a deny into a challenge.
//! It never calls a model or ClickHouse: everything it needs arrives as entity attributes, from
//! the enforcement point or, with awzy-context, from its principals snapshot ([`principals`]) and
//! the resource lookup ([`resources`]). With awzy-challenge, contained accessors are denied
//! ([`containments`]) and enforced challenges are opened and honored once passed ([`challenges`]).

pub mod challenges;
pub mod containments;
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
    AccessRequest, ChallengeKind, Challenges, Decision, DecisionId, DecisionRecord, DelegationCheck, EnforcementMode,
    EntityRef, Verdict,
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

/// What awzy knows about a request beyond its entities, for the PDP to put in each check's context.
#[derive(Default)]
pub struct Facts<'a> {
    /// Who is assigned to open work on the resource (ADR 0046), for `context.assigned`.
    pub assignees: Option<&'a HashSet<String>>,
    /// awzy-reasoner's scores for the subject and the person it acts for, keyed `Type::id`
    /// (ADR 0056), for `context.risk_score`.
    pub risk: Option<&'a std::collections::HashMap<String, i64>>,
}

/// `Resource` attributes added after the first schema, each with a value of its type: a bundle's
/// schema is probed for each, and the ones it doesn't declare are dropped from resource entities.
static OPTIONAL_RESOURCE_ATTRS: std::sync::LazyLock<Vec<(&'static str, serde_json::Value)>> =
    std::sync::LazyLock::new(|| {
        let human = serde_json::json!({ "__entity": { "type": "Human", "id": "probe" } });
        let group = serde_json::json!({ "__entity": { "type": "Group", "id": "probe" } });
        vec![
            ("unclassified", serde_json::json!(true)),
            ("owner", human.clone()),
            ("shared_humans", serde_json::json!([human.clone()])),
            ("shared_groups", serde_json::json!([group.clone()])),
            ("writer_humans", serde_json::json!([human.clone()])),
            ("writer_groups", serde_json::json!([group])),
            // The names before ADR 0073. awzy-context sends both for a release, and a bundle's
            // schema declares one pair, so the other is dropped here.
            ("shared_people", serde_json::json!([human.clone()])),
            ("writer_people", serde_json::json!([human])),
            ("shared_with_everyone", serde_json::json!(true)),
        ]
    });

pub struct Pdp {
    schema: Schema,
    policies: PolicySet,
    authorizer: Authorizer,
    /// Version of the policy bundle in force; 0 for the built-in defaults.
    version: i64,
    /// The optional `Resource` attributes the bundle's schema doesn't declare. A bundle published
    /// before one existed doesn't, and Cedar rejects attributes a schema doesn't declare, so they
    /// are dropped before evaluation.
    unknown_resource_attrs: Vec<&'static str>,
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
        let unknown_resource_attrs = OPTIONAL_RESOURCE_ATTRS
            .iter()
            .filter(|(attr, example)| {
                let mut attrs = serde_json::json!({ "sensitivity": "internal" });
                attrs[*attr] = example.clone();
                let probe = serde_json::json!([{ "uid": { "type": "Resource", "id": "probe" }, "attrs": attrs, "parents": [] }]);
                Entities::from_json_value(probe, Some(&schema)).is_err()
            })
            .map(|(attr, _)| *attr)
            .collect();
        let read = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new("read"));
        let probe = serde_json::json!({ "risk_score": 0, "human_present": true, "assigned": true });
        let knows_assigned = Context::from_json_value(probe, Some((&schema, &read))).is_ok();
        Ok(Self { schema, policies, authorizer: Authorizer::new(), version: 0, unknown_resource_attrs, knows_assigned })
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
        if !self.unknown_resource_attrs.is_empty() {
            for e in json.as_array_mut().into_iter().flatten() {
                if e["uid"]["type"] == awzy_model::entity_type::RESOURCE {
                    if let Some(attrs) = e["attrs"].as_object_mut() {
                        for attr in &self.unknown_resource_attrs {
                            attrs.remove(*attr);
                        }
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
        self.decide_passed(req, entities, assignees, Challenges::default())
    }

    /// [`Self::decide_assigned`] for a request that has passed `passed` challenges: `forbid`
    /// policies that ask only for one of those are set aside, so a passed challenge never lifts
    /// a plain deny or a challenge of another kind.
    pub fn decide_passed(
        &self,
        req: &AccessRequest,
        entities: &Entities,
        assignees: Option<&HashSet<String>>,
        passed: Challenges,
    ) -> anyhow::Result<Decision> {
        self.decide_with(req, entities, &Facts { assignees, risk: None }, passed)
    }

    /// The general form: what awzy knows beyond the entities (`facts`), and the challenges the
    /// request has passed.
    pub fn decide_with(
        &self,
        req: &AccessRequest,
        entities: &Entities,
        facts: &Facts<'_>,
        passed: Challenges,
    ) -> anyhow::Result<Decision> {
        for key in ["on_behalf_of", "human_present", "assigned"] {
            anyhow::ensure!(
                !req.context.contains_key(key),
                "context key {key:?} is set by the PDP; use the request field"
            );
        }
        let mut context = req.context.clone();
        // Each check replaces this with its own principal's score when awzy-reasoner has one
        // (ADR 0056); an enforcement point's value only stands for principals it has never scored.
        context.entry("risk_score").or_insert(serde_json::Value::from(0));
        context.insert("human_present".into(), serde_json::Value::Bool(req.human_present()));
        if let Some(h) = &req.on_behalf_of {
            context.insert("on_behalf_of".into(), serde_json::json!({ "__entity": { "type": h.kind, "id": h.id } }));
        }

        let facts = Facts { assignees: facts.assignees.filter(|_| self.knows_assigned), risk: facts.risk };
        let without_passed;
        let policy_set = if passed.is_empty() {
            &self.policies
        } else {
            without_passed = PolicySet::from_policies(
                self.policies
                    .policies()
                    .filter(|p| {
                        let kind = p.annotation("challenge").and_then(ChallengeKind::parse);
                        !kind.is_some_and(|k| passed.contains(k))
                    })
                    .cloned(),
            )?;
            &without_passed
        };
        let (verdict, policies) = self.check(policy_set, &req.subject, req, &context, entities, &facts)?;
        let on_behalf_of = match &req.on_behalf_of {
            Some(human) => {
                let (verdict, policies) = self.check(policy_set, human, req, &context, entities, &facts)?;
                Some(DelegationCheck { verdict, policies })
            }
            None => None,
        };
        let verdict = on_behalf_of.as_ref().map_or(verdict, |d| verdict.stricter(d.verdict));
        // AppState::check stamps the install's mode; a bare decision is never enforced by default.
        Ok(Decision {
            id: DecisionId::new(),
            verdict,
            policies,
            on_behalf_of,
            mode: EnforcementMode::Monitor,
            challenge_id: None,
        })
    }

    /// One Cedar evaluation with `principal`. Returns the verdict and determining policy IDs.
    fn check(
        &self,
        policy_set: &PolicySet,
        principal: &EntityRef,
        req: &AccessRequest,
        context: &serde_json::Map<String, serde_json::Value>,
        entities: &Entities,
        facts: &Facts<'_>,
    ) -> anyhow::Result<(Verdict, Vec<String>)> {
        let action = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new(&req.action));
        let mut context = context.clone();
        if let Some(assignees) = facts.assignees {
            context.insert("assigned".into(), serde_json::Value::Bool(assignees.contains(&principal.id)));
        }
        if let Some(score) = facts.risk.and_then(|r| r.get(&format!("{}::{}", principal.kind, principal.id))) {
            context.insert("risk_score".into(), serde_json::Value::from(*score));
        }
        let context = Context::from_json_value(serde_json::Value::Object(context), Some((&self.schema, &action)))?;
        let request = Request::new(uid(principal)?, action, uid(&req.resource)?, context, Some(&self.schema))?;
        let evaluating = std::time::Instant::now();
        let response = self.authorizer.is_authorized(&request, policy_set, entities);
        metrics::CEDAR_EVALUATION.observe(evaluating.elapsed());

        let determining: Vec<_> = response.diagnostics().reason().filter_map(|id| policy_set.policy(id)).collect();
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
                    // And only in front of something policy would otherwise allow: passing a
                    // challenge on a request nothing permits would still end in a deny.
                    Some(kinds)
                        if !kinds.is_empty() && self.allowed_without_challenges(policy_set, &request, entities)? =>
                    {
                        Verdict::Challenge { challenges: kinds.into_iter().collect() }
                    }
                    _ => Verdict::Deny,
                }
            }
        };
        Ok((verdict, policies))
    }
}

impl Pdp {
    /// Whether `request` is allowed once every `@challenge` policy is set aside.
    fn allowed_without_challenges(
        &self,
        policy_set: &PolicySet,
        request: &Request,
        entities: &Entities,
    ) -> anyhow::Result<bool> {
        let rest =
            PolicySet::from_policies(policy_set.policies().filter(|p| p.annotation("challenge").is_none()).cloned())?;
        let evaluating = std::time::Instant::now();
        let allowed =
            self.authorizer.is_authorized(request, &rest, entities).decision() == cedar_policy::Decision::Allow;
        metrics::CEDAR_EVALUATION.observe(evaluating.elapsed());
        Ok(allowed)
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
    /// Checks callers' service tokens (ADR 0055). Off in tests and local runs.
    pub service_auth: Arc<awzy_service_auth::Verifier>,
    /// Contained accessors from awzy-challenge, if configured.
    pub containments: Arc<containments::Containments>,
    /// Opens and resolves enforced challenges, if configured.
    pub challenges: Arc<challenges::ChallengeResolver>,
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
            service_auth: Arc::new(awzy_service_auth::Verifier::off()),
            containments: Arc::new(containments::Containments::none()),
            challenges: Arc::new(challenges::ChallengeResolver::none()),
        }
    }

    /// Containments and challenges from awzy-challenge.
    pub fn with_challenges(
        mut self,
        containments: Arc<containments::Containments>,
        challenges: Arc<challenges::ChallengeResolver>,
    ) -> Self {
        self.containments = containments;
        self.challenges = challenges;
        self
    }

    pub fn with_resources(mut self, resources: Arc<resources::Resources>) -> Self {
        self.resources = resources;
        self
    }

    pub fn with_enforcement(mut self, enforcement: Arc<Enforcement>) -> Self {
        self.enforcement = enforcement;
        self
    }

    pub fn with_service_auth(mut self, verifier: awzy_service_auth::Verifier) -> Self {
        self.service_auth = Arc::new(verifier);
        self
    }

    pub fn with_principals(mut self, principals: Arc<principals::Principals>) -> Self {
        self.principals = principals;
        self
    }

    /// Ready once the policy bundle and mode are loaded and, with awzy-context, the first
    /// principals snapshot, and with awzy-challenge, the first containment list.
    pub fn is_ready(&self) -> bool {
        self.enforcement.is_ready() && self.principals.is_ready() && self.containments.is_ready()
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
        let mode = if req.always_enforced { EnforcementMode::Enforce } else { self.enforcement.mode() };
        let who: Vec<&awzy_model::EntityRef> = std::iter::once(&req.subject).chain(req.on_behalf_of.as_ref()).collect();
        let contained = self.containments.find(&who).await.map_err(CheckError::Unavailable)?;
        // With awzy-context, every service account must be registered there (ADR 0033).
        let assembled = if contained.is_empty() {
            Some(
                replay::assemble(&self.principals, &self.resources, req, entities)
                    .await
                    .map_err(CheckError::Unavailable)?,
            )
        } else {
            None
        };
        // Loaded after the lookup, which may wait on awzy-context, so a sync isn't held up.
        let pdp = self.pdp.load_full();
        let mut decision = match assembled {
            // A contained accessor, or a delegated request for a contained person, is denied
            // before any policy is evaluated. The containment IDs explain it.
            None => Decision { policies: contained, ..replay::unregistered() },
            Some(replay::Assembled::Unregistered) => replay::unregistered(),
            Some(replay::Assembled::Ready { entities, assignees, risk }) => {
                let parties = challenges::parties(req, &entities);
                let entities = pdp.entities(entities)?;
                let facts = Facts { assignees: assignees.as_deref(), risk: Some(&risk) };
                let decision = pdp.decide_with(req, &entities, &facts, Challenges::default())?;
                match decision.verdict {
                    // In monitor nothing is sent, so there's nothing to resolve (ADR 0007).
                    Verdict::Challenge { challenges } if mode == EnforcementMode::Enforce => {
                        self.resolve_challenge(&pdp, req, &entities, &facts, decision, challenges, parties).await?
                    }
                    _ => decision,
                }
            }
        };
        decision.mode = mode;
        if let Some(log) = &self.log {
            log.record(DecisionRecord::new(req, &decision, pdp.version(), chrono::Utc::now()));
        }
        Ok(decision)
    }

    /// An enforced challenge: denied if an approver rejected the request, decided again with
    /// what it has passed set aside, and otherwise given the challenge to answer.
    #[expect(clippy::too_many_arguments, reason = "one call site, each argument distinct")]
    async fn resolve_challenge(
        &self,
        pdp: &Pdp,
        req: &AccessRequest,
        entities: &Entities,
        facts: &Facts<'_>,
        mut decision: Decision,
        required: awzy_model::Challenges,
        parties: challenges::Parties,
    ) -> Result<Decision, CheckError> {
        if !self.challenges.is_configured() {
            return Ok(decision);
        }
        let resolution = match self.challenges.resolve(decision.id, req, required, &parties).await {
            Ok(r) => r,
            Err(error) => {
                tracing::warn!(%error, decision_id = %decision.id, "couldn't open a challenge; the verdict stands");
                return Ok(decision);
            }
        };
        if resolution.rejected {
            decision.verdict = Verdict::Deny;
            decision.policies.push(challenges::CHALLENGE_REJECTED.to_string());
            return Ok(decision);
        }
        if !resolution.passed.is_empty() {
            let id = decision.id;
            decision = pdp.decide_with(req, entities, facts, resolution.passed)?;
            decision.id = id;
        }
        if matches!(decision.verdict, Verdict::Challenge { .. }) {
            decision.challenge_id = resolution.challenge_id;
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
///
/// Deciding needs a service token from any workload in the cluster (ADR 0055): enforcement points
/// run anywhere, and the policies decide what they may do. `/metrics` comes from awzy-server, open
/// for the reporter (ADR 0068).
pub fn router(state: AppState) -> Router {
    let guard =
        awzy_service_auth::Guard::new(state.service_auth.clone(), awzy_service_auth::Allowed::AnyServiceAccount);
    Router::new()
        .route("/v1/access/evaluation", post(evaluate))
        .with_state(state.clone())
        .merge(grpc::router(state.clone()))
        .route_layer(axum::middleware::from_fn_with_state(guard, awzy_service_auth::require))
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
    fn a_bundle_from_before_humans_still_sees_who_a_resource_is_shared_with() {
        let old_schema =
            DEFAULT_SCHEMA.replace("shared_humans", "shared_people").replace("writer_humans", "writer_people");
        let policies = r#"
            permit (principal, action == Action::"read", resource)
            when { resource has shared_people && resource.shared_people.contains(principal) };
        "#;
        let human = |id: &str| json!({ "__entity": { "type": "Human", "id": id } });
        // What awzy-context sends while both names are in use (ADR 0073).
        let shared = |pdp: &Pdp| {
            pdp.entities(json!([
                { "uid": { "type": "Human", "id": "sam" }, "attrs": {}, "parents": [] },
                { "uid": { "type": "Resource", "id": "plan" }, "parents": [], "attrs": {
                    "sensitivity": "internal",
                    "shared_people": [human("sam")], "shared_humans": [human("sam")],
                    "writer_people": [], "writer_humans": [] } },
            ]))
            .unwrap()
        };
        let old = Pdp::new(&old_schema, policies).unwrap();
        assert_eq!(old.decide(&read("sam", "plan", 0), &shared(&old)).unwrap().verdict, Verdict::Allow);
        let new = Pdp::with_defaults().unwrap();
        assert!(new.decide(&read("sam", "plan", 0), &shared(&new)).is_ok(), "the old names are dropped, not an error");
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
    fn a_passed_challenge_lifts_only_its_own_kind() {
        let pdp = Pdp::with_defaults().unwrap();
        let entities = fixtures(&pdp);
        let verified = Challenges::of(ChallengeKind::Verification);

        let d = pdp.decide_passed(&read("bob", "design-doc", 90), &entities, None, verified).unwrap();
        assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["engineering-read-internal".to_string()]));

        let approved = Challenges::of(ChallengeKind::Approval);
        let d = pdp.decide_passed(&read("bob", "design-doc", 90), &entities, None, approved).unwrap();
        assert_eq!(d.verdict, Verdict::Challenge { challenges: verified }, "approval isn't verification");

        // A challenge sits on top of a permit; passing one never grants what no policy permits.
        let d = pdp.decide_passed(&read("bob", "board-deck", 90), &entities, None, verified).unwrap();
        assert_eq!(d.verdict, Verdict::Deny);
    }

    #[test]
    fn the_security_team_approves_challenges() {
        let pdp = Pdp::with_defaults().unwrap();
        let approve = |who: &str| AccessRequest { action: "approve".into(), ..read(who, "board-deck", 10) };
        assert_eq!(pdp.decide(&approve("alice"), &fixtures(&pdp)).unwrap().verdict, Verdict::Allow);
        assert_eq!(pdp.decide(&approve("bob"), &fixtures(&pdp)).unwrap().verdict, Verdict::Deny);
    }

    #[test]
    fn an_agent_approves_only_where_a_policy_names_it_and_never_for_its_owner() {
        let approve = |agent: &str, what: &str, approving_for: Option<&str>| {
            let mut req = AccessRequest { action: "approve".into(), ..sa_read(agent, None, what, 10) };
            if let Some(h) = approving_for {
                req.context.insert("approving_for".into(), json!({ "__entity": { "type": "Human", "id": h } }));
            }
            req
        };
        // ops-bot is in the security group, but the base policies let only its people approve.
        let base = Pdp::with_defaults().unwrap();
        assert_eq!(
            base.decide(&approve("ops-bot", "design-doc", Some("bob")), &fixtures(&base)).unwrap().verdict,
            Verdict::Deny
        );

        let overseer = format!(
            "{DEFAULT_POLICIES}\n@id(\"ops-bot-approves\") permit (principal == ServiceAccount::\"ops-bot\", action == Action::\"approve\", resource);"
        );
        let pdp = Pdp::new(DEFAULT_SCHEMA, &overseer).unwrap();
        let verdict = |req| pdp.decide(&req, &fixtures(&pdp)).unwrap().verdict;
        assert_eq!(verdict(approve("ops-bot", "design-doc", Some("bob"))), Verdict::Allow);
        assert_eq!(verdict(approve("ops-bot", "design-doc", Some("alice"))), Verdict::Deny, "alice owns ops-bot");
        assert_eq!(verdict(approve("ops-bot", "design-doc", None)), Verdict::Deny, "no telling whose it is");
        assert_eq!(verdict(approve("ops-bot", "board-deck", Some("bob"))), Verdict::Deny, "crown jewels need a person");
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
    fn an_administrator_at_elevated_risk_touches_a_key_for_high_risk_operations() {
        let pdp = Pdp::with_defaults().unwrap();
        let entities = pdp
            .entities(json!([
                { "uid": { "type": "Group", "id": "it" }, "attrs": {}, "parents": [] },
                { "uid": { "type": "Group", "id": "it-admin" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it" }] },
                { "uid": { "type": "Human", "id": "lee" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it-admin" }] },
                { "uid": { "type": "McpTool", "id": "larkspur-admin/export_directory" },
                  "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
                { "uid": { "type": "McpTool", "id": "larkspur-admin/reset_password" },
                  "attrs": { "sensitivity": "confidential" }, "parents": [] },
            ]))
            .unwrap();
        let call = |tool: &str, risk: i64| AccessRequest {
            action: "call".into(),
            resource: EntityRef::new("McpTool", format!("larkspur-admin/{tool}")),
            ..read("lee", "", risk)
        };
        let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };

        // Routine for IT at ordinary risk.
        let d = pdp.decide(&call("export_directory", 10), &entities).unwrap();
        assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["it-calls-any-tool".to_string()]));
        // Elevated: the high-risk operation needs the key, the ordinary one doesn't.
        let d = pdp.decide(&call("export_directory", 50), &entities).unwrap();
        assert_eq!((d.verdict, d.policies), (verify, vec!["elevated-risk-high-risk-operations-verify".to_string()]));
        assert_eq!(pdp.decide(&call("reset_password", 50), &entities).unwrap().verdict, Verdict::Allow);
        // And touching it is what lets the operation run.
        let passed = Challenges::of(ChallengeKind::Verification);
        let d = pdp.decide_passed(&call("export_directory", 50), &entities, None, passed).unwrap();
        assert_eq!(d.verdict, Verdict::Allow);
    }

    /// Larkspur's shapes (ADR 0061): Sam, a Quarry Labs contractor in engineering; Nora in sales;
    /// SalesGenie in Nora's team; Priya owns the payments design and the strategic plan.
    fn larkspur(pdp: &Pdp) -> Entities {
        let group = |id: &str| json!({ "__entity": { "type": "Group", "id": id } });
        let human = |id: &str| json!({ "__entity": { "type": "Human", "id": id } });
        let doc = |id: &str, tier: &str, owner: &str, groups: &[&str], writers: &[&str]| {
            json!({ "uid": { "type": "Resource", "id": id }, "parents": [], "attrs": {
                "sensitivity": tier, "owner": human(owner),
                "shared_humans": [], "shared_groups": groups.iter().map(|g| group(g)).collect::<Vec<_>>(),
                "writer_humans": [], "writer_groups": writers.iter().map(|g| group(g)).collect::<Vec<_>>(),
                "shared_with_everyone": false } })
        };
        pdp.entities(json!([
            { "uid": { "type": "Human", "id": "sam" }, "attrs": {}, "parents": [{ "type": "Group", "id": "contractors-quarry-labs" }] },
            { "uid": { "type": "Group", "id": "contractors-quarry-labs" }, "attrs": {},
              "parents": [{ "type": "Group", "id": "engineering" }, { "type": "Group", "id": "contractors" }] },
            { "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Group", "id": "contractors" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [{ "type": "Group", "id": "sales-ne" }] },
            { "uid": { "type": "Group", "id": "sales-ne" }, "attrs": {}, "parents": [{ "type": "Group", "id": "sales" }] },
            { "uid": { "type": "Group", "id": "sales" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
              "attrs": { "owner": human("nora"), "ai": true }, "parents": [{ "type": "Group", "id": "sales-ne" }] },
            { "uid": { "type": "Human", "id": "priya" }, "attrs": {}, "parents": [] },
            doc("payments", "confidential", "priya", &["engineering"], &[]),
            doc("plan", "crown_jewels", "priya", &["sales"], &[]),
            doc("export", "confidential", "priya", &["finance"], &[]),
            doc("accounts", "internal", "nora", &["sales-ne"], &["sales-ne"]),
        ]))
        .unwrap()
    }

    fn as_(who: &str, kind: &str, action: &str, what: &str) -> AccessRequest {
        AccessRequest { subject: EntityRef::new(kind, who), action: action.into(), ..read(who, what, 0) }
    }

    #[test]
    fn owners_and_shares_grant_reads_and_writers_grant_writes() {
        let pdp = Pdp::with_defaults().unwrap();
        let e = larkspur(&pdp);
        let nobody_assigned = HashSet::new();
        let decide = |r: AccessRequest| pdp.decide_assigned(&r, &e, Some(&nobody_assigned)).unwrap();
        assert_eq!(decide(as_("nora", "Human", "read", "accounts")).verdict, Verdict::Allow, "shared with her team");
        assert_eq!(decide(as_("nora", "Human", "write", "accounts")).verdict, Verdict::Allow, "her team writes");
        assert_eq!(decide(as_("nora", "Human", "read", "export")).verdict, Verdict::Deny, "not shared with her");
        assert_eq!(decide(as_("sam", "Human", "write", "payments")).verdict, Verdict::Deny, "engineering only reads");
        assert_eq!(decide(as_("priya", "Human", "write", "export")).verdict, Verdict::Allow, "the owner writes");
        assert_eq!(decide(as_("priya", "Human", "approve", "payments")).verdict, Verdict::Allow, "owners approve");
        assert_eq!(decide(as_("nora", "Human", "approve", "payments")).verdict, Verdict::Deny);
    }

    #[test]
    fn the_curious_contractor_needs_the_owners_approval() {
        let pdp = Pdp::with_defaults().unwrap();
        let e = larkspur(&pdp);
        let d = pdp.decide_assigned(&as_("sam", "Human", "read", "payments"), &e, Some(&HashSet::new())).unwrap();
        assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) });
        assert_eq!(d.policies, vec!["contractors-outside-their-work"]);
        let mine = HashSet::from(["sam".to_string()]);
        let d = pdp.decide_assigned(&as_("sam", "Human", "read", "payments"), &e, Some(&mine)).unwrap();
        assert_eq!(d.verdict, Verdict::Allow, "within his statement of work");
        let d = pdp.decide_assigned(&as_("sam", "Human", "read", "export"), &e, Some(&HashSet::new())).unwrap();
        assert_eq!(d.verdict, Verdict::Deny, "not shared with him at all: no challenge to pass");
    }

    #[test]
    fn the_rogue_agent_needs_its_person_and_the_owner() {
        let pdp = Pdp::with_defaults().unwrap();
        let e = larkspur(&pdp);
        let genie = AccessRequest {
            on_behalf_of: Some(EntityRef::new("Human", "nora")),
            human_present: Some(true),
            ..as_("sales-genie", "ServiceAccount", "read", "plan")
        };
        let d = pdp.decide_assigned(&genie, &e, Some(&HashSet::new())).unwrap();
        let both = [ChallengeKind::Justification, ChallengeKind::Approval].into_iter().collect();
        assert_eq!(d.verdict, Verdict::Challenge { challenges: both });
        assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Allow, "Nora herself may read the overshared plan");
        let nora = AccessRequest { on_behalf_of: None, ..as_("nora", "Human", "read", "plan") };
        assert_eq!(pdp.decide_assigned(&nora, &e, Some(&HashSet::new())).unwrap().verdict, Verdict::Allow);
        let accounts = AccessRequest { resource: EntityRef::new("Resource", "accounts"), ..genie };
        assert_eq!(
            pdp.decide_assigned(&accounts, &e, Some(&HashSet::new())).unwrap().verdict,
            Verdict::Allow,
            "internal"
        );
    }

    #[test]
    fn a_challenge_only_stands_in_front_of_something_policy_allows() {
        // Risk over 80 asks for a security key, but nothing permits bob to read the board deck:
        // passing the challenge would still end in a deny, so it's a deny now.
        let d = decide(&read("bob", "board-deck", 90));
        assert_eq!(d.verdict, Verdict::Deny);
        assert_eq!(decide(&read("bob", "design-doc", 90)).verdict.as_str(), "challenge", "permitted, then challenged");
    }

    #[test]
    fn a_bundle_from_before_sharing_still_loads_its_resources() {
        let mut old_schema = DEFAULT_SCHEMA.to_string();
        let start = old_schema.find("  // Who owns it in its source system").unwrap();
        let end = old_schema[start..].find("};").unwrap() + start;
        old_schema.replace_range(start..end, "");
        let policies = "permit (principal, action == Action::\"read\", resource);";
        let pdp = Pdp::new(&old_schema, policies).unwrap();
        assert!(pdp
            .entities(json!([{ "uid": { "type": "Resource", "id": "doc" }, "parents": [], "attrs": {
            "sensitivity": "internal", "owner": { "__entity": { "type": "Human", "id": "p" } },
            "shared_groups": [], "shared_with_everyone": true } }]))
            .is_ok());
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
        assert_eq!((alice.verdict, alice.policies), (Verdict::Deny, vec!["deactivated-humans".to_string()]));

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
    async fn each_check_sees_its_own_principals_risk_score() {
        let principals = Arc::new(principals::Principals::awaiting_sync());
        principals.load_with_risk(
            1,
            vec![
                json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "engineering" }] }),
                json!({ "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] }),
                json!({ "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                        "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true },
                        "parents": [{ "type": "Group", "id": "engineering" }] }),
            ],
            [("Human::bob".to_string(), 90), ("ServiceAccount::sales-genie".to_string(), 10)].into(),
        );
        let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_principals(principals);
        let doc = json!([{ "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
        let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };

        // The enforcement point says 0; awzy's score for bob is 90, and high-risk-verify fires.
        let d = state.check(&read("bob", "design-doc", 0), doc.clone()).await.unwrap();
        assert_eq!((d.verdict, d.policies), (verify, vec!["high-risk-verify".to_string()]));

        // SalesGenie (10) acting for bob (90): its own check passes, bob's is challenged, and the
        // stricter verdict stands.
        let req = AccessRequest { human_present: Some(true), ..sa_read("sales-genie", Some("bob"), "design-doc", 0) };
        let d = state.check(&req, doc.clone()).await.unwrap();
        assert_eq!(d.verdict, verify);
        assert_eq!(d.on_behalf_of.unwrap().verdict, verify);
        assert!(!d.policies.contains(&"high-risk-verify".to_string()), "the agent's own score is low");

        // Someone awzy hasn't scored keeps what the enforcement point sent, as before.
        let carol = json!([{ "uid": { "type": "Human", "id": "carol" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
                          { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
        assert_eq!(state.check(&read("carol", "design-doc", 5), carol).await.unwrap().verdict, Verdict::Allow);
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
