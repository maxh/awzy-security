//! awzy-pdp: the policy decision point on the hot path.
//!
//! Evaluates one action on one resource against Cedar policies and returns allow, deny, or
//! challenge. A `forbid` policy annotated `@challenge("<kind>")` turns a deny into a challenge.
//! It never calls a model or ClickHouse: everything it needs arrives as entity attributes, from
//! the enforcement point or, with awzy-context, from its principals snapshot ([`principals`]) and
//! the resource lookup ([`resources`]). With awzy-challenge, contained principals are denied
//! ([`containments`]) and enforced challenges are opened and honored once passed ([`challenges`]).

pub mod app_types;
pub mod challenges;
pub mod containments;
mod decision_log;
mod enforcement;
mod grpc;
pub mod metrics;
mod prepared;
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
    ChallengeKind, Check, Decision, DecisionId, DecisionRecord, DelegationCheck, EnforcementMode, EntityRef, Verdict,
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
    /// awzy-reasoner's scores for the principal and the person it acts for, keyed `Type::id`
    /// (ADR 0056), for `context.risk_score`.
    pub risk: Option<&'a std::collections::HashMap<String, i64>>,
    /// People's automation scores, keyed the same way (ADR 0085), for `context.automation_score`.
    pub automation: Option<&'a std::collections::HashMap<String, i64>>,
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
            ("writer_humans", serde_json::json!([human])),
            ("writer_groups", serde_json::json!([group])),
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
    /// Whether the schema declares `context.mcp_client` for `call` (ADR 0081). Same reason: a
    /// bundle published before it existed doesn't, and the gateway sends it anyway.
    knows_mcp_client: bool,
    /// Whether the schema declares `context.automation_score` (ADR 0085). Same reason.
    knows_automation: bool,
    /// Whether the schema declares `context.support_areas` (ADR 0095). Without it the key is
    /// dropped, so a support session's checks match no policy and are denied.
    knows_support_areas: bool,
    parsed: prepared::ParsedEntities,
    prepared: prepared::PreparedPolicies,
    /// The bundle's app-owned entity types (ADR 0108), whose attributes come from the
    /// enforcement point.
    app_types: std::collections::BTreeSet<String>,
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
        // Every type a bundle adds is marked app-owned, and none of awzy's are (ADR 0108).
        let app_types = app_types::app_types(schema)?;
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
        let call = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new("call"));
        let probe = serde_json::json!({ "risk_score": 0, "human_present": true, "mcp_client": "probe" });
        let knows_mcp_client = Context::from_json_value(probe, Some((&schema, &call))).is_ok();
        let probe = serde_json::json!({ "risk_score": 0, "human_present": true, "automation_score": 0 });
        let knows_automation = Context::from_json_value(probe, Some((&schema, &read))).is_ok();
        let view = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new("view"));
        let probe = serde_json::json!({ "risk_score": 0, "human_present": true, "support_areas": [] });
        let knows_support_areas = Context::from_json_value(probe, Some((&schema, &view))).is_ok();
        Ok(Self {
            prepared: prepared::PreparedPolicies::new(&policies)?,
            parsed: prepared::ParsedEntities::new(&schema)?,
            schema,
            policies,
            authorizer: Authorizer::new(),
            version: 0,
            unknown_resource_attrs,
            knows_assigned,
            knows_mcp_client,
            knows_support_areas,
            knows_automation,
            app_types,
        })
    }

    /// Whether `kind` is one of the bundle's app-owned entity types (ADR 0108).
    pub fn is_app_type(&self, kind: &str) -> bool {
        self.app_types.contains(kind)
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

    /// Parses Cedar entity JSON (principals, teams, resources) against the schema.
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
        match json {
            serde_json::Value::Array(items) => self.parsed.entities(&self.schema, items),
            other => Ok(Entities::from_json_value(other, Some(&self.schema))?),
        }
    }

    /// Decides one request. A delegated request (`on_behalf_of` set) is checked twice: once
    /// for the principal and once as the person it acts for. The stricter verdict wins, so a
    /// service account can never do more than the person it acts for.
    pub fn decide(&self, req: &Check, entities: &Entities) -> anyhow::Result<Decision> {
        self.decide_assigned(req, entities, None)
    }

    /// [`Self::decide`], with who is assigned to work on the resource when that's known
    /// (ADR 0046). Each check then sees `context.assigned` for its own principal: the account for
    /// the principal's check, the person for the delegated one.
    pub fn decide_assigned(
        &self,
        req: &Check,
        entities: &Entities,
        assignees: Option<&HashSet<String>>,
    ) -> anyhow::Result<Decision> {
        self.decide_passed(req, entities, assignees, &HashSet::new())
    }

    /// [`Self::decide_assigned`] for a request whose challenges from the `@challenge` policies in
    /// `passed` (by ID) it has passed: those are set aside, and only those, so a passed challenge
    /// never lifts a plain deny, or a challenge another policy asks for (ADR 0080).
    pub fn decide_passed(
        &self,
        req: &Check,
        entities: &Entities,
        assignees: Option<&HashSet<String>>,
        passed: &HashSet<String>,
    ) -> anyhow::Result<Decision> {
        self.decide_with(req, entities, &Facts { assignees, ..Facts::default() }, passed)
    }

    /// The general form: what awzy knows beyond the entities (`facts`), and the `@challenge`
    /// policies whose challenges the request has passed.
    pub fn decide_with(
        &self,
        req: &Check,
        entities: &Entities,
        facts: &Facts<'_>,
        passed: &HashSet<String>,
    ) -> anyhow::Result<Decision> {
        for key in ["on_behalf_of", "human_present", "assigned", "automation_score"] {
            anyhow::ensure!(
                !req.context.contains_key(key),
                "context key {key:?} is set by the PDP; use the request field"
            );
        }
        let mut context = req.context.clone();
        if !self.knows_mcp_client {
            context.remove("mcp_client");
        }
        if !self.knows_support_areas {
            context.remove("support_areas");
        }
        // Each check replaces this with its own principal's score when awzy-reasoner has one
        // (ADR 0056); an enforcement point's value only stands for principals it has never scored.
        context.entry("risk_score").or_insert(serde_json::Value::from(0));
        context.insert("human_present".into(), serde_json::Value::Bool(req.human_present()));
        if let Some(h) = &req.on_behalf_of {
            context.insert("on_behalf_of".into(), serde_json::json!({ "__entity": { "type": h.kind, "id": h.id } }));
        }

        let facts = Facts {
            assignees: facts.assignees.filter(|_| self.knows_assigned),
            risk: facts.risk,
            automation: facts.automation.filter(|_| self.knows_automation),
        };
        let without_passed;
        let policy_set = if passed.is_empty() {
            &self.policies
        } else {
            without_passed = PolicySet::from_policies(
                self.policies
                    .policies()
                    .filter(|p| !(p.annotation("challenge").is_some() && passed.contains(&policy_id(p))))
                    .cloned(),
            )?;
            &without_passed
        };
        let (verdict, policies) = self.check(policy_set, &req.principal, req, &context, entities, &facts)?;
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
        req: &Check,
        context: &serde_json::Map<String, serde_json::Value>,
        entities: &Entities,
        facts: &Facts<'_>,
    ) -> anyhow::Result<(Verdict, Vec<String>)> {
        let action = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new(&req.action));
        let mut context = context.clone();
        if let Some(assignees) = facts.assignees {
            context.insert("assigned".into(), serde_json::Value::Bool(assignees.contains(&principal.id)));
        }
        let key = format!("{}::{}", principal.kind, principal.id);
        if let Some(score) = facts.risk.and_then(|r| r.get(&key)) {
            context.insert("risk_score".into(), serde_json::Value::from(*score));
        }
        if let Some(score) = facts.automation.and_then(|a| a.get(&key)) {
            context.insert("automation_score".into(), serde_json::Value::from(*score));
        }
        let context = prepared::context(&self.schema, &action, context)?;
        let request = Request::new(uid(principal)?, action, uid(&req.resource)?, context, Some(&self.schema))?;
        let evaluating = std::time::Instant::now();
        let response = self.authorizer.is_authorized(&request, policy_set, entities);
        metrics::CEDAR_EVALUATION.observe(evaluating.elapsed());

        let determining: Vec<_> = response.diagnostics().reason().collect();
        // Cedar reports them in no particular order; sort so explanations are stable.
        let mut policies: Vec<String> = determining.iter().map(|id| self.prepared.id(id)).collect();
        policies.sort();
        let verdict = match response.decision() {
            cedar_policy::Decision::Allow => Verdict::Allow,
            cedar_policy::Decision::Deny => {
                // Challenge only when every forbid that fired asks for one; then all of them are required.
                let kinds: Option<Vec<ChallengeKind>> =
                    determining.iter().map(|id| self.prepared.challenge(id)).collect();
                match kinds {
                    // And only in front of something policy would otherwise allow: passing a
                    // challenge on a request nothing permits would still end in a deny.
                    Some(kinds) if !kinds.is_empty() && self.allowed_without_challenges(&request, entities) => {
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
    fn allowed_without_challenges(&self, request: &Request, entities: &Entities) -> bool {
        let rest = &self.prepared.without_challenges;
        let evaluating = std::time::Instant::now();
        let allowed =
            self.authorizer.is_authorized(request, rest, entities).decision() == cedar_policy::Decision::Allow;
        metrics::CEDAR_EVALUATION.observe(evaluating.elapsed());
        allowed
    }
}

impl Pdp {
    /// The `@challenge` policies behind a challenge verdict, with the kind each asks for: for the
    /// principal and for the person it acts for. A grant is recorded against these (ADR 0080).
    pub fn asking_policies(&self, decision: &Decision) -> Vec<(String, ChallengeKind)> {
        let ids: HashSet<&String> =
            decision.policies.iter().chain(decision.on_behalf_of.iter().flat_map(|d| &d.policies)).collect();
        let mut asking: Vec<(String, ChallengeKind)> = self
            .policies
            .policies()
            .filter_map(|p| {
                let kind = p.annotation("challenge").and_then(ChallengeKind::parse)?;
                let id = policy_id(p);
                ids.contains(&id).then_some((id, kind))
            })
            .collect();
        asking.sort_by(|a, b| a.0.cmp(&b.0));
        asking.dedup();
        asking
    }
}

/// A policy's ID as decisions report it: its `@id` annotation, else Cedar's.
fn policy_id(p: &cedar_policy::Policy) -> String {
    p.annotation("id").map(str::to_string).unwrap_or_else(|| p.id().to_string())
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
    /// Contained principals from awzy-challenge, if configured.
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
    pub async fn check(&self, req: &Check, entities: serde_json::Value) -> Result<Decision, CheckError> {
        let started = std::time::Instant::now();
        let result = self.check_inner(req, entities).await;
        self.metrics.record(result.as_ref().ok().map(|d| &d.verdict), started.elapsed());
        result
    }

    async fn check_inner(&self, req: &Check, entities: serde_json::Value) -> Result<Decision, CheckError> {
        if !self.is_ready() {
            return Err(CheckError::NotReady);
        }
        let mode = if req.always_enforced { EnforcementMode::Enforce } else { self.enforcement.mode() };
        let who: Vec<&awzy_model::EntityRef> =
            std::iter::once(&req.principal).chain(req.on_behalf_of.as_ref()).collect();
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
        let attributes_from = match &assembled {
            Some(replay::Assembled::Ready { attributes_from, .. }) => *attributes_from,
            _ => awzy_model::AttributesFrom::EnforcementPoint,
        };
        let mut decision = match assembled {
            // A contained principal, or a delegated request for a contained person, is denied
            // before any policy is evaluated. The containment IDs explain it.
            None => Decision { policies: contained, ..replay::unregistered() },
            Some(replay::Assembled::Unregistered) => replay::unregistered(),
            Some(replay::Assembled::Ready { entities, assignees, risk, automation, .. }) => {
                let parties = challenges::parties(req, &entities);
                let entities = pdp.entities(entities)?;
                let facts = Facts { assignees: assignees.as_deref(), risk: Some(&risk), automation: Some(&automation) };
                let decision = pdp.decide_with(req, &entities, &facts, &HashSet::new())?;
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
            log.record(
                DecisionRecord::new(req, &decision, pdp.version(), chrono::Utc::now())
                    .with_attributes_from(attributes_from),
            );
        }
        Ok(decision)
    }

    /// An enforced challenge: denied if an approver rejected the request, decided again with
    /// what it has passed set aside, and otherwise given the challenge to answer.
    #[expect(clippy::too_many_arguments, reason = "one call site, each argument distinct")]
    async fn resolve_challenge(
        &self,
        pdp: &Pdp,
        req: &Check,
        entities: &Entities,
        facts: &Facts<'_>,
        mut decision: Decision,
        required: awzy_model::Challenges,
        parties: challenges::Parties,
    ) -> Result<Decision, CheckError> {
        if !self.challenges.is_configured() {
            return Ok(decision);
        }
        let asking = pdp.asking_policies(&decision);
        let resolution = match self.challenges.resolve(decision.id, req, required, &asking, &parties).await {
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
        if !resolution.passed_policies.is_empty() {
            let id = decision.id;
            decision = pdp.decide_with(req, entities, facts, &resolution.passed_policies)?;
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
    pub request: Check,
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
mod tests;
