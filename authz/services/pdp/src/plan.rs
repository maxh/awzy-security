//! Query plans (ADR 0113): which resources of a type a principal may act on, as a filter the app
//! runs in its own data layer, from Cedar's partial evaluation with the resource unknown.

pub mod filter;
mod log;
pub mod translate;

use std::{
    collections::{BTreeSet, HashMap},
    str::FromStr,
    sync::Mutex,
    time::{Duration, Instant},
};

use awzy_model::{ChallengeKind, EnforcementMode, EntityRef, PlanId};
use cedar_policy::{pst, Effect, Entities, EntityId, EntityTypeName, EntityUid, Request, RestrictedExpression};
use serde::{Deserialize, Serialize};

pub use filter::{Cmp, Filter, Literal};
pub use log::{PlanLog, DEFAULT_PLANS_TOPIC};

use crate::{policy_id, prepared, Facts, Pdp};

/// How long a plan may be used, and pinned across a pagination cursor.
pub const MAX_AGE: Duration = Duration::from_secs(60);

/// What an app asks: an evaluation's principal, person, action, and context, and a resource
/// type with no ID.
#[derive(Debug, Clone, Deserialize)]
pub struct PlanRequest {
    pub principal: EntityRef,
    #[serde(default)]
    pub on_behalf_of: Option<EntityRef>,
    pub action: String,
    pub resource_type: String,
    #[serde(default)]
    pub context: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub human_present: Option<bool>,
    /// The first page's plan, to keep paginating with (ADR 0113).
    #[serde(default, rename = "plan_qid")]
    pub plan_id: Option<PlanId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// Every row: no filter is needed.
    AlwaysAllowed,
    /// No row: the query can short-circuit.
    AlwaysDenied,
    /// A challenge that depends only on context fires for every row. `filter` is what applies
    /// once it's passed.
    Challenge,
    /// The rows `filter` matches.
    Conditional,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::AlwaysAllowed => "always_allowed",
            Outcome::AlwaysDenied => "always_denied",
            Outcome::Challenge => "challenge",
            Outcome::Conditional => "conditional",
        }
    }
}

/// What a plan says, before the serving PDP adds its QID, mode, and versions.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub outcome: Outcome,
    pub filter: Filter,
    pub challenges: BTreeSet<&'static str>,
    /// A challenge that depends on the resource counts as a forbid here; passing it could show
    /// more rows.
    pub challenge_could_widen: bool,
    /// Policies that could decide some row.
    pub policies: Vec<String>,
}

/// One principal's side of a plan.
struct Side {
    filter: Filter,
    challenges: BTreeSet<&'static str>,
    widened: bool,
    policies: BTreeSet<String>,
}

/// A principal's own entity and the groups it's in, from the request's entities.
struct EntityAncestry<'a>(&'a Entities);

impl translate::Ancestry for EntityAncestry<'_> {
    fn with_ancestors(&self, entity: &EntityRef) -> Vec<EntityRef> {
        let mut out = vec![entity.clone()];
        if let Ok(uid) = crate::uid(entity) {
            out.extend(
                self.0
                    .ancestors(&uid)
                    .into_iter()
                    .flatten()
                    .map(|a| EntityRef::new(a.type_name().to_string(), a.id().unescaped())),
            );
        }
        out
    }
}

fn kind_name(kind: ChallengeKind) -> &'static str {
    match kind {
        ChallengeKind::Justification => "justification",
        ChallengeKind::Verification => "verification",
        ChallengeKind::Approval => "approval",
        ChallengeKind::Biometric => "biometric",
    }
}

impl Pdp {
    /// The plan for `req`: one principal's filter, or for a delegated request the AND of both
    /// principals' (ADR 0016), as a check is the stricter of two.
    pub fn plan(&self, req: &PlanRequest, entities: &Entities, facts: &Facts<'_>) -> anyhow::Result<Planned> {
        anyhow::ensure!(
            self.schema.entity_types().any(|t| t.to_string() == req.resource_type),
            "{:?} isn't an entity type in the policy bundle's schema",
            req.resource_type
        );
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
        context.entry("risk_score").or_insert(serde_json::Value::from(0));
        let human_present = req.human_present.unwrap_or(req.principal.kind == awzy_model::entity_type::HUMAN);
        context.insert("human_present".into(), serde_json::Value::Bool(human_present));
        if let Some(h) = &req.on_behalf_of {
            context.insert("on_behalf_of".into(), serde_json::json!({ "__entity": { "type": h.kind, "id": h.id } }));
        }

        let mine = self.side(&req.principal, req, &context, entities, facts)?;
        let theirs = match &req.on_behalf_of {
            Some(human) => Some(self.side(human, req, &context, entities, facts)?),
            None => None,
        };
        let mut filter = mine.filter;
        let mut challenges = mine.challenges;
        let mut widened = mine.widened;
        let mut policies = mine.policies;
        if let Some(t) = theirs {
            filter = Filter::and([filter, t.filter]);
            challenges.extend(t.challenges);
            widened |= t.widened;
            policies.extend(t.policies);
        }
        let outcome = match (&filter, challenges.is_empty()) {
            (Filter::False, _) => Outcome::AlwaysDenied,
            (_, false) => Outcome::Challenge,
            (Filter::True, true) => Outcome::AlwaysAllowed,
            _ => Outcome::Conditional,
        };
        if outcome == Outcome::AlwaysDenied {
            challenges.clear();
        }
        Ok(Planned {
            outcome,
            filter,
            challenges,
            challenge_could_widen: widened,
            policies: policies.into_iter().collect(),
        })
    }

    /// One Cedar partial evaluation with `principal` and the resource unknown but typed.
    fn side(
        &self,
        principal: &EntityRef,
        req: &PlanRequest,
        context: &serde_json::Map<String, serde_json::Value>,
        entities: &Entities,
        facts: &Facts<'_>,
    ) -> anyhow::Result<Side> {
        let action = EntityUid::from_type_name_and_id(EntityTypeName::from_str("Action")?, EntityId::new(&req.action));
        let mut context = context.clone();
        let key = format!("{}::{}", principal.kind, principal.id);
        if let Some(score) = facts.risk.and_then(|r| r.get(&key)) {
            context.insert("risk_score".into(), serde_json::Value::from(*score));
        }
        if let Some(score) = facts.automation.filter(|_| self.knows_automation).and_then(|a| a.get(&key)) {
            context.insert("automation_score".into(), serde_json::Value::from(*score));
        }
        let mut cedar_context = prepared::context(&self.schema, &action, context)?;
        // Worked out for each of awzy's resources (ADR 0046), so unknown here: a filter that depends
        // on it says so. App-owned types never have it, in a check or a plan.
        if self.knows_assigned && req.resource_type == awzy_model::entity_type::RESOURCE {
            cedar_context =
                cedar_context.merge([("assigned".to_string(), RestrictedExpression::new_unknown("assigned"))])?;
        }
        let request = Request::builder()
            .principal(crate::uid(principal)?)
            .action(action)
            .unknown_resource_with_type(EntityTypeName::from_str(&req.resource_type)?)
            .context(cedar_context)
            .build();
        let policy_set = &self.prepared.for_action(&req.action).all;
        let response = self.authorizer.is_authorized_partial(&request, policy_set, entities);
        let ancestry = EntityAncestry(entities);

        let (mut permits, mut forbids, mut asking) = (Vec::new(), Vec::new(), Vec::new());
        let mut challenges = BTreeSet::new();
        let mut policies = BTreeSet::new();
        for residual in response.all_residuals() {
            let condition = match residual.to_pst()? {
                pst::Policy::Static(p) => match p.body().clauses().as_slice() {
                    [pst::Clause::When(expr)] => translate::filter(expr, &ancestry),
                    clauses => Filter::Unmappable { cedar: format!("{clauses:?}") },
                },
                other => Filter::Unmappable { cedar: format!("{other:?}") },
            };
            if condition == Filter::False {
                continue;
            }
            policies.insert(policy_id(&residual));
            match (residual.effect(), residual.annotation("challenge").and_then(ChallengeKind::parse)) {
                (Effect::Permit, _) => permits.push(condition),
                // A challenge that fires for every row is the plan's to say up front.
                (Effect::Forbid, Some(kind)) if condition == Filter::True => {
                    challenges.insert(kind_name(kind));
                }
                (Effect::Forbid, Some(_)) => asking.push(condition),
                (Effect::Forbid, None) => forbids.push(condition),
            }
        }
        let widened = !asking.is_empty();
        let filter =
            Filter::and([Filter::or(permits), Filter::not(Filter::or(forbids)), Filter::not(Filter::or(asking))]);
        Ok(Side { filter, challenges, widened, policies })
    }
}

/// Plans by their inputs, and by QID for a paginating app, valid while what they rest on holds.
#[derive(Default)]
pub struct PlanCache {
    entries: Mutex<HashMap<String, Cached>>,
    by_id: Mutex<HashMap<PlanId, String>>,
}

/// The versions a plan rests on: the policy bundle, the principals snapshot (which risk scores
/// move), and the containments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Versions {
    pub policy: i64,
    pub principals: i64,
    pub containments: u64,
}

#[derive(Clone)]
struct Cached {
    id: PlanId,
    planned: Planned,
    versions: Versions,
    made: Instant,
}

/// Plans kept before the oldest are dropped.
const CACHE_MAX: usize = 10_000;

impl PlanCache {
    /// A cached plan for these inputs that still rests on `now`, or with `pinned` (the first
    /// page's QID), that plan whatever the versions, while it's younger than [`MAX_AGE`].
    /// Containments always count: a contained principal is denied before the cache is asked.
    pub fn get(&self, key: &str, pinned: Option<PlanId>, now: Versions) -> Option<(PlanId, Planned, Versions, bool)> {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = pinned {
            let by_id = self.by_id.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(c) = by_id.get(&id).and_then(|k| entries.get(k)).filter(|c| c.id == id) {
                if c.made.elapsed() < MAX_AGE && c.versions.containments == now.containments {
                    return Some((c.id, c.planned.clone(), c.versions, true));
                }
            }
        }
        let c = entries.get(key)?;
        (c.versions == now && c.made.elapsed() < MAX_AGE).then(|| (c.id, c.planned.clone(), c.versions, false))
    }

    pub fn put(&self, key: String, id: PlanId, planned: Planned, versions: Versions) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        let mut by_id = self.by_id.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= CACHE_MAX {
            entries.retain(|_, c| c.made.elapsed() < MAX_AGE);
            by_id.retain(|_, k| entries.contains_key(k));
            if entries.len() >= CACHE_MAX {
                entries.clear();
                by_id.clear();
            }
        }
        by_id.insert(id, key.clone());
        entries.insert(key, Cached { id, planned, versions, made: Instant::now() });
    }
}

/// The cache key for a request's inputs.
pub fn cache_key(req: &PlanRequest) -> String {
    serde_json::json!([req.principal, req.on_behalf_of, req.action, req.resource_type, req.context, req.human_present])
        .to_string()
}

/// A short, stable hash of a filter, which plans with the same filter share.
pub fn filter_hash(filter: &Filter) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(serde_json::to_vec(filter).unwrap_or_default());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// The plan as the API answers it.
#[derive(Debug, Clone, Serialize)]
pub struct PlanResponse {
    pub qid: PlanId,
    pub outcome: Outcome,
    pub filter: Filter,
    pub challenges: Vec<&'static str>,
    pub challenge_could_widen: bool,
    /// Some row must be checked one by one: the filter has an `unknown` or `unmappable` node.
    pub needs_row_checks: bool,
    /// The resource attributes the filter reads.
    pub attributes: Vec<String>,
    pub policies: Vec<String>,
    /// In monitor the enforcement point allows every row and records what the filter would hide.
    pub mode: EnforcementMode,
    pub policy_version: i64,
    pub principals_version: i64,
    pub max_age_secs: u64,
    /// The same plan as the `plan_qid` the request carried, for the next page.
    pub pinned: bool,
}

impl PlanResponse {
    pub fn new(id: PlanId, planned: Planned, mode: EnforcementMode, versions: Versions, pinned: bool) -> Self {
        Self {
            qid: id,
            outcome: planned.outcome,
            needs_row_checks: planned.filter.needs_row_checks(),
            attributes: planned.filter.attributes(),
            filter: planned.filter,
            challenges: planned.challenges.into_iter().collect(),
            challenge_could_widen: planned.challenge_could_widen,
            policies: planned.policies,
            mode,
            policy_version: versions.policy,
            principals_version: versions.principals,
            max_age_secs: MAX_AGE.as_secs(),
            pinned,
        }
    }

    /// For a principal denied before any policy: contained, or unregistered.
    pub fn denied(policies: Vec<String>, mode: EnforcementMode, versions: Versions) -> Self {
        let planned = Planned {
            outcome: Outcome::AlwaysDenied,
            filter: Filter::False,
            challenges: BTreeSet::new(),
            challenge_could_widen: false,
            policies,
        };
        Self::new(PlanId::new(), planned, mode, versions, false)
    }
}

impl crate::AppState {
    /// The plan for `req` (ADR 0113), from the cache when what it rests on still holds, recorded
    /// in the plan log. A contained or unregistered principal gets `always_denied`.
    pub async fn plan(
        &self,
        req: &PlanRequest,
        entities: serde_json::Value,
    ) -> Result<PlanResponse, crate::CheckError> {
        use crate::CheckError;
        if !self.is_ready() {
            return Err(CheckError::NotReady);
        }
        let pdp = self.pdp.load_full();
        let versions = Versions {
            policy: pdp.version(),
            principals: self.principals.version(),
            containments: self.containments.epoch(),
        };
        let mode = self.enforcement.mode_for(&req.action, &EntityRef::new(&req.resource_type, ""));
        let who: Vec<&EntityRef> = std::iter::once(&req.principal).chain(req.on_behalf_of.as_ref()).collect();
        let contained = self.containments.find(&who).await.map_err(CheckError::Unavailable)?;
        if !contained.is_empty() {
            return Ok(self.recorded(req, PlanResponse::denied(contained, mode, versions)));
        }
        if self.principals.is_unregistered(&req.principal) {
            let denied = PlanResponse::denied(vec![crate::principals::UNREGISTERED.to_string()], mode, versions);
            return Ok(self.recorded(req, denied));
        }
        let key = cache_key(req);
        if let Some((id, planned, made_under, pinned)) = self.plans.get(&key, req.plan_id, versions) {
            return Ok(PlanResponse::new(id, planned, mode, made_under, pinned));
        }
        let merged = self.principals.merge(entities, &who);
        let risk = self.principals.risk_of(&who);
        let automation = self.principals.automation_of(&who);
        let facts = Facts { assignees: None, risk: Some(&risk), automation: Some(&automation) };
        let planned = pdp.plan(req, &pdp.entities(merged)?, &facts)?;
        let id = PlanId::new();
        self.plans.put(key, id, planned.clone(), versions);
        Ok(self.recorded(req, PlanResponse::new(id, planned, mode, versions, false)))
    }

    fn recorded(&self, req: &PlanRequest, res: PlanResponse) -> PlanResponse {
        if let Some(log) = &self.plan_log {
            log.record(awzy_model::PlanRecord {
                id: res.qid,
                at: chrono::Utc::now(),
                principal_type: req.principal.kind.clone(),
                principal_id: req.principal.id.clone(),
                on_behalf_of_type: req.on_behalf_of.as_ref().map(|h| h.kind.clone()).unwrap_or_default(),
                on_behalf_of_id: req.on_behalf_of.as_ref().map(|h| h.id.clone()).unwrap_or_default(),
                action: req.action.clone(),
                resource_type: req.resource_type.clone(),
                outcome: res.outcome.as_str().to_string(),
                mode: res.mode,
                challenge: res.challenges.join(","),
                filter: serde_json::to_string(&res.filter).unwrap_or_default(),
                filter_hash: filter_hash(&res.filter),
                policies: res.policies.clone(),
                policy_version: res.policy_version,
                principals_version: res.principals_version,
                context: serde_json::Value::Object(req.context.clone()).to_string(),
            });
        }
        res
    }
}

/// `POST /v1/access/plan`: a plan request, with the principals' entities inline like an
/// evaluation's.
#[derive(Deserialize)]
pub(crate) struct PlanBody {
    #[serde(flatten)]
    request: PlanRequest,
    #[serde(default)]
    entities: Vec<serde_json::Value>,
}

pub(crate) async fn http(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    axum::Json(body): axum::Json<PlanBody>,
) -> Result<axum::Json<PlanResponse>, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;
    state.plan(&body.request, serde_json::Value::Array(body.entities)).await.map(axum::Json).map_err(|e| match e {
        crate::CheckError::NotReady | crate::CheckError::Unavailable(_) => {
            (StatusCode::SERVICE_UNAVAILABLE, e.to_string())
        }
        crate::CheckError::Invalid(_) => (StatusCode::BAD_REQUEST, e.to_string()),
    })
}

#[cfg(test)]
mod tests;
