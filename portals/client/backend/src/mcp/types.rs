//! The MCP tools' parameters and results, as their JSON schemas describe them to clients. IDs
//! are QIDs and times are RFC 3339 UTC strings.

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::decisions::Verdict;

#[derive(Deserialize, JsonSchema, Default)]
#[schemars(crate = "rmcp::schemars")]
pub struct SearchDecisionsParams {
    /// Only decisions whose principal (user or agent) has this ID, e.g. "alice".
    pub principal_id: Option<String>,
    /// Only decisions on the resource with this ID.
    pub resource_id: Option<String>,
    pub verdict: Option<Verdict>,
    /// monitor: decisions that were only recorded (a deny is a "would deny"); enforce: applied.
    pub mode: Option<ModeParam>,
    /// RFC 3339 timestamp. Defaults to 24 hours ago; at most 30 days ago.
    pub since: Option<String>,
    /// How many to return, newest first. Defaults to 20, at most 200.
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema, Clone, Copy)]
#[schemars(crate = "rmcp::schemars")]
#[serde(rename_all = "lowercase")]
pub enum ModeParam {
    Monitor,
    Enforce,
}

impl From<ModeParam> for awzy_model::EnforcementMode {
    fn from(m: ModeParam) -> Self {
        match m {
            ModeParam::Monitor => Self::Monitor,
            ModeParam::Enforce => Self::Enforce,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ExplainDecisionParams {
    /// The decision's ID, `decision_<uuid>`, as the PDP returned it.
    pub decision_qid: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CheckAccessParams {
    /// `Human` or `ServiceAccount`.
    pub principal_type: String,
    /// The principal's ID, as your IdP or the service-account registry names it, e.g. "alice".
    pub principal_id: String,
    /// The action, e.g. "read", "write", "call", "view", "administer".
    pub action: String,
    /// `Resource`, `McpTool`, or `PortalArea`.
    pub resource_type: String,
    /// The resource's ID, e.g. a document's ID, "drive/delete_file", or "policies".
    pub resource_id: String,
    /// For a service account acting for a person: that person's ID.
    pub on_behalf_of_id: Option<String>,
    /// Whether a person is there to answer a challenge. Defaults as a real check does: yes for a
    /// person, no for a service account.
    pub human_present: Option<bool>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CheckOut {
    /// allow, deny, or challenge.
    pub verdict: String,
    /// For a challenge: every kind that must be passed.
    pub challenges: Vec<String>,
    /// The policies that produced it.
    pub policies: Vec<String>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CheckAccessOut {
    /// The verdict, the stricter of the principal's and the person's when it acts for one.
    pub verdict: String,
    pub challenges: Vec<String>,
    pub policies: Vec<String>,
    /// The second check, as the person a service account acts for.
    pub on_behalf_of: Option<CheckOut>,
    /// What this is and isn't.
    pub note: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GetPrincipalParams {
    /// `Human` or `ServiceAccount`.
    pub principal_type: String,
    /// The principal's ID, e.g. "alice" or "sales-genie".
    pub principal_id: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PrincipalOut {
    /// The principal as the PDPs see it, in Cedar's entity shape: attributes (`active`, a service
    /// account's `owner` and `ai`) and the groups it's in. None when awzy doesn't know it.
    pub entity: Option<serde_json::Value>,
    /// Every group above it, nested groups included.
    pub groups: Vec<String>,
    /// Its risk score from awzy-reasoner, 0 to 100, when it has one.
    pub risk_score: Option<i64>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GetResourceParams {
    /// The resource's ID in awzy-context, e.g. a document's ID from its source system.
    pub resource_id: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ResourceOut {
    /// The resource and the folders above it, nearest first, in Cedar's entity shape: sensitivity,
    /// owner, and sharing. Empty when awzy doesn't know it.
    pub entities: Vec<serde_json::Value>,
    /// Who is assigned to open work that links it or a folder above it.
    pub assignees: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PrincipalActivityParams {
    /// `Human` or `ServiceAccount`.
    pub principal_type: String,
    /// The principal's ID, e.g. "alice" or "sales-genie".
    pub principal_id: String,
    /// How many hours back to look. Defaults to 24, at most 168 (a week).
    pub hours: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PrincipalActivityOut {
    /// Decisions by verdict and action, resources touched, agents that acted for it, and events.
    pub activity: serde_json::Value,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RequestLiftParams {
    /// The containment's QID, `containment_<uuid>`.
    pub containment_qid: String,
    /// Why it should be lifted, for the person who decides.
    pub reason: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct RequestLiftOut {
    /// The containment's principal, still contained.
    pub principal: String,
    /// What happens next.
    pub next_step: String,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DryRunPolicyParams {
    /// A draft bundle's QID, `policy_bundle_<uuid>`, as `propose_policy` returned it.
    pub draft_qid: String,
    /// How many days of recent requests to replay. Defaults to 7, at most 30.
    pub days: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DryRunOut {
    /// What would change: the verdicts that differ under the draft, how often, and examples.
    pub dry_run: serde_json::Value,
}

#[derive(Deserialize, JsonSchema, Default)]
#[schemars(crate = "rmcp::schemars")]
pub struct ListPolicyBundlesParams {
    /// How many to return, newest first. Defaults to 20.
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ProposePolicyParams {
    /// The complete Cedar policy set for the new bundle (not a diff).
    pub policies: String,
    /// The Cedar schema. Defaults to the active bundle's schema.
    pub schema: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DecisionSummary {
    /// `decision_<uuid>`
    pub id: String,
    /// RFC 3339, UTC.
    pub at: String,
    /// `Type::id`, e.g. `Human::alice` or `ServiceAccount::sales-genie`.
    pub principal: String,
    /// The person a service account acted for, if the request was delegated.
    pub on_behalf_of: Option<String>,
    /// Whether a person was there to answer a challenge.
    pub human_present: bool,
    pub action: String,
    pub resource: String,
    /// allow, deny, or challenge: what policy decided. For a delegated request, the stricter of
    /// both checks.
    pub verdict: String,
    /// monitor or enforce. In monitor the verdict was not applied: a deny is a "would deny".
    pub mode: String,
    /// Every challenge required, when the verdict is challenge.
    pub challenges: Vec<String>,
    /// IDs of the policies that determined the principal's own check.
    pub policies: Vec<String>,
    /// For a delegated request: the person's own check (allow, deny, or challenge).
    pub on_behalf_of_verdict: Option<String>,
    /// The policy bundle version in force; 0 means the built-in defaults.
    pub policy_version: i64,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct DecisionList {
    pub decisions: Vec<DecisionSummary>,
    /// More decisions match than were returned.
    pub has_more: bool,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PolicySourceOut {
    pub id: String,
    pub source: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Explanation {
    pub decision: DecisionSummary,
    /// The request context the PDP saw, as JSON text; null when a policy hides it (ADR 0103).
    pub context: Option<String>,
    /// One sentence on why the verdict came out as it did.
    pub summary: String,
    /// Policies that determined the principal's own check.
    pub determining_policies: Vec<PolicySourceOut>,
    /// For a delegated request: policies that determined the person's own check.
    pub on_behalf_of_policies: Vec<PolicySourceOut>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct PolicyBundleOut {
    /// `policy_bundle_<uuid>`
    pub id: String,
    pub version: i64,
    /// draft, active, or superseded.
    pub status: String,
    pub created_by: String,
    pub created_at: Option<String>,
    pub published_at: Option<String>,
    /// Cedar policies; included for the active bundle.
    pub policies: Option<String>,
    /// Cedar schema; included for the active bundle.
    pub schema: Option<String>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ActiveBundle {
    /// None when no bundle has been published and the PDPs enforce the built-in defaults.
    pub bundle: Option<PolicyBundleOut>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BundleList {
    pub bundles: Vec<PolicyBundleOut>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct EnforcementOut {
    /// monitor (decisions are recorded, enforcement points allow) or enforce.
    pub mode: String,
    /// Who last switched it and when (RFC 3339); null if never switched.
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
    /// Counts over the last 24 hours.
    pub last_24h_total: u64,
    pub last_24h_would_deny: u64,
    pub last_24h_would_challenge: u64,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct Proposal {
    /// The saved draft, unless validation failed.
    pub draft: Option<PolicyBundleOut>,
    /// Cedar parse or schema validation errors. Fix them and propose again.
    pub validation_error: Option<String>,
    /// What happens next.
    pub next_step: String,
}

#[derive(Deserialize, JsonSchema, Default)]
#[schemars(crate = "rmcp::schemars")]
pub struct ListInvestigationsParams {
    /// Only these: needs_review (a person has to decide), done, or closed.
    pub status: Option<String>,
    /// How many to return, newest first. Defaults to 20, at most 200.
    pub limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct GetInvestigationParams {
    /// `investigation_<uuid>`
    pub investigation_qid: String,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InvestigationList {
    /// Each with its principal, verdict, risk, summary, cited evidence, containment, and status.
    pub investigations: Vec<serde_json::Value>,
}

#[derive(Serialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct InvestigationOut {
    /// None when there's no such investigation.
    pub investigation: Option<serde_json::Value>,
}
