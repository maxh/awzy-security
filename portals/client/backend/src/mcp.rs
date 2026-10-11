//! MCP server for the client's own AI agents, served at `/mcp` (streamable HTTP).
//!
//! Lets a security team's agent explain decisions, search the decision log, read policy
//! bundles, and propose a policy change. Each tool calls the same operations as the GraphQL
//! resolvers, so a tool can't see or do more than the matching portal action. Tools that
//! would change enforcement only create drafts: a human publishes them in the portal.
//!
//! Callers sign in like portal users (crate::access): usually an agent with a token-exchange
//! token, checked as the service account acting for its person, with `context.client` "mcp".
//! Each call is recorded with awzy-intake as the agent's activity (ADR 0084).

use std::sync::Arc;

use awzy_authz_proto::policy::v1::PolicyBundle as WireBundle;
use awzy_model::DecisionId;
use axum::{http::request::Parts, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use rmcp::{
    handler::server::{
        tool::Extension,
        wrapper::{Json, Parameters},
    },
    schemars::JsonSchema,
    tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData,
};
use serde::{Deserialize, Serialize};

use crate::{
    access::{self, PortalAction, PortalArea, Surface, Viewer},
    decisions::{self, DecisionRow, SearchParams, Verdict},
    policies::{self, DraftOutcome},
    Services,
};

#[derive(Clone)]
pub struct ClientPortalMcp {
    services: Services,
}

/// Serves MCP at `/mcp`. `allowed_hosts` are the Host headers to accept (DNS-rebinding
/// protection); empty keeps rmcp's default of loopback hosts only.
pub fn router(services: Services, allowed_hosts: Vec<String>) -> Router {
    let mut config = StreamableHttpServerConfig::default();
    config.json_response = true;
    // Stateless: no session lives in one pod, so a rollout or a second replica drops no client
    // (ADR 0072). Every tool is a single request and response.
    config.legacy_session_mode = false;
    if !allowed_hosts.is_empty() {
        config.allowed_hosts = allowed_hosts;
    }
    let service = StreamableHttpService::new(
        move || Ok(ClientPortalMcp { services: services.clone() }),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new().nest_service("/mcp", service)
}

// ---- Tool inputs and outputs. IDs are QIDs and times are RFC 3339 UTC strings.

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
    /// The request context the PDP saw, as JSON text.
    pub context: String,
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

#[tool_router(server_handler)]
impl ClientPortalMcp {
    #[tool(
        name = "list_investigations",
        description = "awzy-reasoner's investigations of principals it flagged, newest first: what the model found, the evidence it cited, and what awzy's checks let it do. Filter by status: needs_review, done, or closed. Read-only."
    )]
    async fn list_investigations(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<ListInvestigationsParams>,
    ) -> Result<Json<InvestigationList>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Investigations).await?;
        self.record(&viewer, "list_investigations");
        if let Some(s) = p.status.as_deref().filter(|s| !["needs_review", "done", "closed"].contains(s)) {
            return Err(ErrorData::invalid_params(format!("status is needs_review, done, or closed, not {s:?}"), None));
        }
        let limit = p.limit.unwrap_or(20).min(200) as i32;
        let list = crate::investigations::list(self.context()?, p.status.as_deref(), limit).await.map_err(internal)?;
        Ok(Json(InvestigationList { investigations: list.iter().map(crate::investigations::to_json).collect() }))
    }

    #[tool(
        name = "get_investigation",
        description = "One investigation by its QID: its principal, why it was opened, the verdict, risk, summary, the evidence it cited, what awzy's checks changed, and whether a person has closed it. Read-only."
    )]
    async fn get_investigation(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<GetInvestigationParams>,
    ) -> Result<Json<InvestigationOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Investigations).await?;
        self.record(&viewer, "get_investigation");
        let found = crate::investigations::get(self.context()?, &p.investigation_qid).await.map_err(internal)?;
        Ok(Json(InvestigationOut { investigation: found.as_ref().map(crate::investigations::to_json) }))
    }

    #[tool(
        name = "search_decisions",
        description = "Search authorization decisions, newest first. Filter by principal, resource, verdict, and time (last 24 hours by default, at most 30 days)."
    )]
    async fn search_decisions(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<SearchDecisionsParams>,
    ) -> Result<Json<DecisionList>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        self.record(&viewer, "search_decisions");
        let since = p.since.as_deref().map(parse_time).transpose()?;
        let page = decisions::search(
            self.clickhouse()?,
            SearchParams {
                principal_id: p.principal_id,
                resource_id: p.resource_id,
                verdict: p.verdict,
                mode: p.mode.map(Into::into),
                since,
                after: None,
                limit: p.limit.unwrap_or(20) as usize,
            },
        )
        .await
        .map_err(internal)?;
        Ok(Json(DecisionList { decisions: page.rows.iter().map(summary_of).collect(), has_more: page.has_next }))
    }

    #[tool(
        name = "explain_decision",
        description = "Explain one decision: what was asked, the verdict, and the source of the policies that produced it, from the policy bundle in force at the time."
    )]
    async fn explain_decision(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<ExplainDecisionParams>,
    ) -> Result<Json<Explanation>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        self.record(&viewer, "explain_decision");
        let id: DecisionId =
            p.decision_qid.parse().map_err(|e: awzy_model::QidError| ErrorData::invalid_params(e.to_string(), None))?;
        let row = decisions::find(self.clickhouse()?, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| ErrorData::invalid_params(format!("{id} is not in the decision log"), None))?;
        let policy = self.services.policy.clone();
        let own = decisions::explain(policy.clone(), row.policy_version, &row.policies).await.map_err(internal)?;
        let human =
            decisions::explain(policy, row.policy_version, &row.on_behalf_of_policies).await.map_err(internal)?;
        Ok(Json(Explanation {
            decision: summary_of(&row),
            context: row.context.clone(),
            summary: explain_sentence(&row, &own),
            determining_policies: own.into_iter().map(|p| PolicySourceOut { id: p.id, source: p.source }).collect(),
            on_behalf_of_policies: human.into_iter().map(|p| PolicySourceOut { id: p.id, source: p.source }).collect(),
        }))
    }

    #[tool(
        name = "check_access",
        description = "What-if: would this principal be allowed this action on this resource now, under the active policies, with the people, groups, risk, and resources awzy knows today? Decides like a PDP but records nothing and opens no challenge. Read-only."
    )]
    async fn check_access(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<CheckAccessParams>,
    ) -> Result<Json<CheckAccessOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
        self.record(&viewer, "check_access");
        let pdp = match policies::active(self.policy()?).await.map_err(internal)? {
            Some(b) => awzy_pdp::Pdp::new(&b.schema, &b.policies).map_err(internal)?,
            None => awzy_pdp::Pdp::with_defaults().map_err(internal)?,
        };
        let check = awzy_model::Check {
            principal: awzy_model::EntityRef::new(&p.principal_type, &p.principal_id),
            action: p.action,
            resource: awzy_model::EntityRef::new(&p.resource_type, &p.resource_id),
            context: serde_json::Map::new(),
            on_behalf_of: p.on_behalf_of_id.map(|h| awzy_model::EntityRef::new(awzy_model::entity_type::HUMAN, h)),
            human_present: p.human_present,
            always_enforced: true,
        };
        let replayer = crate::dry_run::replayer(&self.services).await.map_err(internal)?;
        let d = replayer.decide(&pdp, &check, serde_json::json!([])).await.map_err(internal)?;
        let out = |v: awzy_model::Verdict, policies: Vec<String>| CheckOut {
            verdict: v.as_str().to_string(),
            challenges: match v {
                awzy_model::Verdict::Challenge { challenges } => {
                    challenges.iter().map(|k| k.as_str().to_string()).collect()
                }
                _ => vec![],
            },
            policies,
        };
        let own = out(d.verdict, d.policies);
        Ok(Json(CheckAccessOut {
            verdict: own.verdict,
            challenges: own.challenges,
            policies: own.policies,
            on_behalf_of: d.on_behalf_of.map(|o| out(o.verdict, o.policies)),
            note: "A what-if under the active bundle: nothing was logged, and no challenge was opened. A real check \
                   also sees what its enforcement point sends, and the install's mode decides whether it's applied."
                .into(),
        }))
    }

    #[tool(
        name = "get_principal",
        description = "A person or service account as the PDPs see it: its attributes, the groups it's in (nested included), and its current risk score. Read-only."
    )]
    async fn get_principal(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<GetPrincipalParams>,
    ) -> Result<Json<PrincipalOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Directory).await?;
        self.record(&viewer, "get_principal");
        let snapshot: serde_json::Value = self.context()?.get("/v1/principals").await.map_err(internal)?;
        Ok(Json(principal_from_snapshot(&snapshot, &p.principal_type, &p.principal_id)))
    }

    #[tool(
        name = "get_resource",
        description = "A resource as the PDPs see it: its sensitivity, owner, and sharing, the folders above it, and who is assigned to work that links it. Read-only."
    )]
    async fn get_resource(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<GetResourceParams>,
    ) -> Result<Json<ResourceOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Directory).await?;
        self.record(&viewer, "get_resource");
        let path = format!("/v1/resources/{}/entities", urlencode(&p.resource_id));
        let (status, body) = self.context()?.call("GET", &path, None, None).await.map_err(internal)?;
        if status == 404 {
            return Ok(Json(ResourceOut { entities: vec![], assignees: vec![] }));
        }
        if status != 200 {
            return Err(internal(anyhow::anyhow!(
                "awzy-context answered {status}: {}",
                String::from_utf8_lossy(&body)
            )));
        }
        let found: serde_json::Value = serde_json::from_slice(&body).map_err(|e| internal(e.into()))?;
        let strings = |v: &serde_json::Value| -> Vec<String> {
            v.as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect()
        };
        Ok(Json(ResourceOut {
            entities: found["entities"].as_array().cloned().unwrap_or_default(),
            assignees: strings(&found["assignees"]),
        }))
    }

    #[tool(
        name = "principal_activity",
        description = "What one person or service account did recently: its decisions by verdict and action, how many resources it touched, which agents acted for it, and its events from apps and audit logs. The last 24 hours by default, at most a week. Read-only."
    )]
    async fn principal_activity(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<PrincipalActivityParams>,
    ) -> Result<Json<PrincipalActivityOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        self.record(&viewer, "principal_activity");
        let hours = i64::from(p.hours.unwrap_or(24).clamp(1, 168));
        let since = Utc::now() - chrono::Duration::hours(hours);
        let a = decisions::principal_activity(self.clickhouse()?, &p.principal_type, &p.principal_id, since)
            .await
            .map_err(internal)?;
        Ok(Json(PrincipalActivityOut { activity: serde_json::to_value(a).map_err(|e| internal(e.into()))? }))
    }

    #[tool(
        name = "request_containment_lift",
        description = "Ask for a containment to be lifted, with a reason. This lifts nothing: the security team is told, and a person decides in the client portal."
    )]
    async fn request_containment_lift(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<RequestLiftParams>,
    ) -> Result<Json<RequestLiftOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Challenges).await?;
        self.record(&viewer, "request_containment_lift");
        let mut client =
            self.services.challenge.clone().ok_or_else(|| {
                ErrorData::internal_error("awzy-challenge is not configured (AWZY_CHALLENGE_ADDR)", None)
            })?;
        let req = awzy_authz_proto::challenge::v1::RequestLiftRequest {
            containment_qid: p.containment_qid,
            requested_by_actor: Some(viewer.actor().into()),
            reason: p.reason,
        };
        let res =
            client.request_lift(req).await.map_err(|s| ErrorData::invalid_request(s.message().to_string(), None))?;
        let principal = res
            .into_inner()
            .containment
            .and_then(|c| c.principal)
            .map(|r| format!("{}::{}", r.r#type, r.id))
            .unwrap_or_default();
        Ok(Json(RequestLiftOut {
            principal,
            next_step: "Nothing is lifted yet. The security team has been told, and decides on the client portal's \
                        Challenges page."
                .into(),
        }))
    }

    #[tool(
        name = "dry_run_policy",
        description = "Replay recent requests under the active policies and a draft (from propose_policy), with today's people and resources, and report every verdict the draft would change, most frequent first. Read-only."
    )]
    async fn dry_run_policy(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<DryRunPolicyParams>,
    ) -> Result<Json<DryRunOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
        self.record(&viewer, "dry_run_policy");
        self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        let run = crate::dry_run::run(&self.services, &p.draft_qid, p.days).await.map_err(internal)?;
        Ok(Json(DryRunOut { dry_run: crate::dry_run::to_json(&run) }))
    }

    #[tool(
        name = "get_active_policy_bundle",
        description = "The Cedar policies and schema the PDPs currently enforce."
    )]
    async fn get_active_policy_bundle(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<Json<ActiveBundle>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
        self.record(&viewer, "get_active_policy_bundle");
        let bundle = policies::active(self.policy()?).await.map_err(internal)?;
        Ok(Json(ActiveBundle { bundle: bundle.map(|b| bundle_out(b, true)) }))
    }

    #[tool(
        name = "get_enforcement",
        description = "Whether awzy is enforcing verdicts or only monitoring them, and over the last 24 hours how many decisions would have been denied or challenged. Read-only: switching modes is done by a person in the client portal."
    )]
    async fn get_enforcement(&self, Extension(parts): Extension<Parts>) -> Result<Json<EnforcementOut>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Enforcement).await?;
        self.record(&viewer, "get_enforcement");
        let setting = crate::enforcement::current(self.policy()?, crate::enforcement::EnforcementScope::Install)
            .await
            .map_err(internal)?;
        let (_, counts) = decisions::mode_counts(self.clickhouse()?, None).await.map_err(internal)?;
        Ok(Json(EnforcementOut {
            mode: awzy_model::EnforcementMode::from(setting.mode()).as_str().to_string(),
            changed_by: Some(setting.changed_by.clone()).filter(|s| !s.is_empty()),
            changed_at: setting
                .changed_at
                .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
                .map(rfc3339),
            last_24h_total: counts.total,
            last_24h_would_deny: counts.would_deny,
            last_24h_would_challenge: counts.would_challenge,
        }))
    }

    #[tool(
        name = "list_policy_bundles",
        description = "Recent policy bundles, newest first, with status and who created or published them."
    )]
    async fn list_policy_bundles(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<ListPolicyBundlesParams>,
    ) -> Result<Json<BundleList>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
        self.record(&viewer, "list_policy_bundles");
        let limit = p.limit.unwrap_or(20).min(500) as i32;
        let bundles = policies::list(self.policy()?, limit).await.map_err(internal)?;
        Ok(Json(BundleList { bundles: bundles.into_iter().map(|b| bundle_out(b, false)).collect() }))
    }

    #[tool(
        name = "propose_policy",
        description = "Validate a new Cedar policy set and save it as a draft bundle. This never changes enforcement: a human must review and publish the draft in the client portal."
    )]
    async fn propose_policy(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<ProposePolicyParams>,
    ) -> Result<Json<Proposal>, ErrorData> {
        let viewer = self.allow(&parts, PortalAction::Administer, PortalArea::Policies).await?;
        self.record(&viewer, "propose_policy");
        let schema = match p.schema {
            Some(s) => s,
            None => policies::active(self.policy()?)
                .await
                .map_err(internal)?
                .map(|b| b.schema)
                .unwrap_or_else(|| awzy_pdp::DEFAULT_SCHEMA.to_string()),
        };
        Ok(Json(match policies::create_draft(self.policy()?, schema, p.policies, &viewer.actor()).await.map_err(internal)? {
            DraftOutcome::Created(b) => Proposal {
                next_step: format!("Draft version {} is saved. A human must publish it in the client portal before it is enforced.", b.version),
                draft: Some(bundle_out(*b, false)),
                validation_error: None,
            },
            DraftOutcome::Invalid(msg) => Proposal {
                draft: None,
                validation_error: Some(msg),
                next_step: "Nothing was saved. Fix the validation errors and propose again.".into(),
            },
        }))
    }
}

impl ClientPortalMcp {
    /// The signed-in caller, if the PDP lets them do `action` in `area`.
    async fn allow(&self, parts: &Parts, action: PortalAction, area: PortalArea) -> Result<Viewer, ErrorData> {
        let viewer = parts
            .extensions
            .get::<Viewer>()
            .cloned()
            .ok_or_else(|| ErrorData::invalid_request("sign in first", None))?;
        match access::authorize(&self.services, &viewer, Surface::Mcp, action, area).await {
            Ok(()) => Ok(viewer),
            Err(e @ (access::AccessError::Denied { .. } | access::AccessError::Challenged { .. })) => {
                Err(ErrorData::invalid_request(e.to_string(), None))
            }
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }

    /// Records the tool call as the viewer's activity in awzy-intake (ADR 0084), once the PDP
    /// allowed it. Best effort, and never waits.
    fn record(&self, viewer: &Viewer, tool: &str) {
        let tool = awzy_model::EntityRef::new(awzy_model::entity_type::MCP_TOOL, format!("awzy-portal/{tool}"));
        let activity =
            awzy_activity::Activity::now(viewer.subject.clone(), viewer.on_behalf_of.clone(), "call", Some(tool))
                .with_attribute("client", "mcp");
        self.services.activity.record(activity);
    }

    fn clickhouse(&self) -> Result<&::clickhouse::Client, ErrorData> {
        self.services
            .clickhouse
            .as_ref()
            .ok_or_else(|| ErrorData::internal_error("ClickHouse is not configured (AWZY_CLICKHOUSE_URL)", None))
    }

    fn context(&self) -> Result<&crate::directory::ContextClient, ErrorData> {
        self.services
            .context
            .as_ref()
            .ok_or_else(|| ErrorData::internal_error("awzy-context is not configured (AWZY_CONTEXT_ADDR)", None))
    }

    fn policy(&self) -> Result<crate::PolicyClient, ErrorData> {
        self.services
            .policy
            .clone()
            .ok_or_else(|| ErrorData::internal_error("the policy service is not configured (AWZY_POLICY_ADDR)", None))
    }
}

/// One principal from awzy-context's snapshot, with its groups up the hierarchy and its risk.
fn principal_from_snapshot(snapshot: &serde_json::Value, kind: &str, id: &str) -> PrincipalOut {
    let entities = snapshot["entities"].as_array().cloned().unwrap_or_default();
    let find =
        |kind: &str, id: &str| entities.iter().find(|e| e["uid"]["type"] == kind && e["uid"]["id"] == id).cloned();
    let entity = find(kind, id);
    let mut groups: Vec<String> = Vec::new();
    let mut queue: Vec<serde_json::Value> =
        entity.iter().flat_map(|e| e["parents"].as_array().cloned().unwrap_or_default()).collect();
    while let Some(parent) = queue.pop() {
        let Some(name) = parent["id"].as_str() else { continue };
        if groups.iter().any(|g| g == name) {
            continue;
        }
        groups.push(name.to_string());
        if let Some(group) = find("Group", name) {
            queue.extend(group["parents"].as_array().cloned().unwrap_or_default());
        }
    }
    groups.sort();
    let risk_score = snapshot["risk"][format!("{kind}::{id}")].as_i64();
    PrincipalOut { entity, groups, risk_score }
}

/// A path segment, percent-encoded.
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn internal(e: anyhow::Error) -> ErrorData {
    ErrorData::internal_error(e.to_string(), None)
}

fn parse_time(s: &str) -> Result<DateTime<Utc>, ErrorData> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|_| ErrorData::invalid_params(format!("{s:?} is not an RFC 3339 timestamp"), None))
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// One sentence on why the verdict came out as it did. For a delegated request, says when the
/// person's own check is what decided it.
pub(crate) fn explain_sentence(row: &DecisionRow, own: &[decisions::PolicySource]) -> String {
    let human = format!("{}::{}", row.on_behalf_of_type, row.on_behalf_of_id);
    if !row.on_behalf_of_id.is_empty() && row.on_behalf_of_verdict == "deny" && row.verdict == "deny" {
        let subject_alone = if row.policies.is_empty() {
            "and the service account couldn't either"
        } else {
            "even though the service account could"
        };
        return format!("Denied: {human} couldn't do this themselves, {subject_alone}. A service account acting for someone gets at most what they could do.");
    }
    match (row.verdict.as_str(), own.is_empty()) {
        ("deny", true) => "Denied by default: no policy permitted this action.".to_string(),
        ("allow", _) if row.on_behalf_of_id.is_empty() => format!("Allowed by {}.", ids(own)),
        ("allow", _) => format!("Allowed by {}, and {human} could do it themselves.", ids(own)),
        ("challenge", _) => format!(
            "Allowed only after {} challenges, required by {}.",
            decisions::split_challenges(&row.challenge).join(" and "),
            ids(own)
        ),
        _ => format!("Denied by {}.", ids(own)),
    }
}

fn ids(policies: &[decisions::PolicySource]) -> String {
    policies.iter().map(|p| format!("`{}`", p.id)).collect::<Vec<_>>().join(", ")
}

pub(crate) fn summary_of(row: &DecisionRow) -> DecisionSummary {
    DecisionSummary {
        id: DecisionId::from_uuid(row.id).to_string(),
        at: rfc3339(row.at),
        principal: format!("{}::{}", row.principal_type, row.principal_id),
        on_behalf_of: (!row.on_behalf_of_id.is_empty())
            .then(|| format!("{}::{}", row.on_behalf_of_type, row.on_behalf_of_id)),
        human_present: row.human_present,
        action: row.action.clone(),
        resource: format!("{}::{}", row.resource_type, row.resource_id),
        verdict: row.verdict.clone(),
        mode: row.mode.clone(),
        challenges: decisions::split_challenges(&row.challenge).into_iter().map(str::to_string).collect(),
        policies: row.policies.clone(),
        on_behalf_of_verdict: (!row.on_behalf_of_verdict.is_empty()).then(|| row.on_behalf_of_verdict.clone()),
        policy_version: row.policy_version,
    }
}

pub(crate) fn bundle_out(b: WireBundle, with_sources: bool) -> PolicyBundleOut {
    let ts = |t: Option<awzy_authz_proto::prost_types::Timestamp>| {
        t.and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32)).map(rfc3339)
    };
    PolicyBundleOut {
        status: b.status().as_str_name().trim_start_matches("BUNDLE_STATUS_").to_lowercase(),
        id: b.qid,
        version: b.version,
        created_by: b.created_by,
        created_at: ts(b.created_at),
        published_at: ts(b.published_at),
        policies: with_sources.then_some(b.policies),
        schema: with_sources.then_some(b.schema),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::{
        policies::tests::{serve_fake_policy, FakePolicy},
        testing,
    };

    async fn services() -> Services {
        Services {
            policy: Some(serve_fake_policy(FakePolicy::default()).await),
            challenge: None,
            clickhouse: None,
            pdp: Some(testing::pdp().await),
            context: None,
            bot: None,
            activity: Default::default(),
        }
    }

    async fn server() -> ClientPortalMcp {
        ClientPortalMcp { services: services().await }
    }

    /// Request parts as the sign-in middleware leaves them for `viewer`.
    fn signed_in(viewer: Viewer) -> Extension<Parts> {
        let (mut parts, ()) = Request::new(()).into_parts();
        parts.extensions.insert(viewer);
        Extension(parts)
    }

    fn alice() -> Extension<Parts> {
        signed_in(testing::human("alice", "security"))
    }

    #[test]
    fn a_principal_comes_with_every_group_above_it_and_its_risk() {
        let snapshot = serde_json::json!({
            "entities": [
                { "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "appsec" }] },
                { "uid": { "type": "Group", "id": "appsec" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
                { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
            ],
            "risk": { "Human::bob": 42 },
        });
        let bob = principal_from_snapshot(&snapshot, "Human", "bob");
        assert_eq!((bob.groups, bob.risk_score), (vec!["appsec".to_string(), "security".to_string()], Some(42)));
        assert_eq!(bob.entity.unwrap()["attrs"]["active"], true);
        let nobody = principal_from_snapshot(&snapshot, "Human", "nobody");
        assert!(nobody.entity.is_none() && nobody.groups.is_empty() && nobody.risk_score.is_none());
    }

    #[tokio::test]
    async fn check_access_decides_without_recording_or_challenging() {
        let server = server().await;
        let ask = |principal_type: &str, principal_id: &str, on_behalf_of_id: Option<&str>| CheckAccessParams {
            principal_type: principal_type.into(),
            principal_id: principal_id.into(),
            action: "view".into(),
            resource_type: "PortalArea".into(),
            resource_id: "challenges".into(),
            on_behalf_of_id: on_behalf_of_id.map(Into::into),
            human_present: None,
        };
        let Json(bob) = server.check_access(alice(), Parameters(ask("Human", "bob", None))).await.unwrap();
        assert_eq!((bob.verdict.as_str(), bob.policies), ("allow", vec!["humans-answer-their-challenges".to_string()]));
        let Json(agent) =
            server.check_access(alice(), Parameters(ask("ServiceAccount", "sales-genie", Some("bob")))).await.unwrap();
        assert_eq!(agent.verdict, "deny", "no policy lets an agent view challenges");
        assert_eq!(agent.on_behalf_of.unwrap().verdict, "allow", "bob could");

        // Only those who may view policies may ask.
        let bob = signed_in(testing::human("bob", "engineering"));
        let Err(err) = server.check_access(bob, Parameters(ask("Human", "bob", None))).await else {
            panic!("bob may not view policies")
        };
        assert!(err.message.contains("may not view policies"), "{}", err.message);
    }

    #[tokio::test]
    async fn propose_policy_only_creates_drafts() {
        let mcp = server().await;
        assert!(mcp.get_active_policy_bundle(alice()).await.unwrap().0.bundle.is_none());

        let bad = mcp
            .propose_policy(alice(), Parameters(ProposePolicyParams { policies: "permit (".into(), schema: None }))
            .await
            .unwrap()
            .0;
        assert!(bad.draft.is_none() && bad.validation_error.is_some());

        let good = mcp
            .propose_policy(
                alice(),
                Parameters(ProposePolicyParams { policies: awzy_pdp::DEFAULT_POLICIES.into(), schema: None }),
            )
            .await
            .unwrap()
            .0;
        let draft = good.draft.unwrap();
        assert_eq!(draft.status, "draft");
        assert_eq!(draft.created_by, "Human::alice");
        assert!(draft.id.starts_with("policy_bundle_"));
        assert!(mcp.get_active_policy_bundle(alice()).await.unwrap().0.bundle.is_none(), "proposing never publishes");

        let bob = signed_in(testing::human("bob", "engineering"));
        let Err(err) =
            mcp.propose_policy(bob, Parameters(ProposePolicyParams { policies: String::new(), schema: None })).await
        else {
            panic!("bob isn't in the security group");
        };
        assert!(err.message.starts_with("bob may not administer policies"), "{}", err.message);
    }

    #[test]
    fn explains_a_delegated_denial_by_the_persons_check() {
        let row = DecisionRow {
            id: uuid::Uuid::now_v7(),
            at: chrono::Utc::now(),
            principal_type: "ServiceAccount".into(),
            principal_id: "ops-bot".into(),
            on_behalf_of_type: "Human".into(),
            on_behalf_of_id: "bob".into(),
            human_present: true,
            action: "read".into(),
            resource_type: "Resource".into(),
            resource_id: "board-deck".into(),
            verdict: "deny".into(),
            mode: "enforce".into(),
            challenge: String::new(),
            policies: vec!["security-team-read".into()],
            on_behalf_of_verdict: "deny".into(),
            on_behalf_of_policies: vec![],
            policy_version: 0,
            context: "{}".into(),
        };
        let own = vec![decisions::PolicySource { id: "security-team-read".into(), source: String::new() }];
        let s = explain_sentence(&row, &own);
        assert!(
            s.starts_with("Denied: Human::bob couldn't do this themselves, even though the service account could"),
            "{s}"
        );
        assert_eq!(summary_of(&row).on_behalf_of.as_deref(), Some("Human::bob"));
    }

    #[tokio::test]
    async fn explain_rejects_ids_that_are_not_decisions() {
        let mcp = server().await;
        let Err(err) = mcp
            .explain_decision(alice(), Parameters(ExplainDecisionParams { decision_qid: "policy_bundle_0199".into() }))
            .await
        else {
            panic!("a policy bundle ID must not be accepted as a decision ID");
        };
        assert!(err.message.contains("is not a decision ID"), "{}", err.message);
    }

    const SECRET_ENV: &str = "AWZY_TEST_PORTAL_SECRET";

    fn token_config(audience: &str) -> awzy_identity::TokenConfig {
        std::env::set_var(SECRET_ENV, "test-secret");
        awzy_identity::TokenConfig {
            issuer: "https://idp.test".into(),
            audience: audience.into(),
            jwks_ca_file: None,
            subject_claim: None,
            key: awzy_identity::TokenKey::Hs256SecretEnv(SECRET_ENV.into()),
        }
    }

    fn agents() -> Vec<awzy_identity::ServiceAccountConfig> {
        vec![awzy_identity::ServiceAccountConfig {
            id: "sec-agent".into(),
            owner: "alice".into(),
            ai: true,
            groups: vec!["security".into()],
        }]
    }

    /// The portal router, with an HS256 test key: tokens for `awzy` on GraphQL, `awzy-mcp` on MCP.
    async fn app() -> Router {
        let portal = Some("https://portal.test".to_string());
        let auth = access::Auth::new(token_config("awzy"), agents(), "portal".into(), portal.clone()).unwrap();
        let mcp = access::Auth::new_for_mcp(token_config("awzy-mcp"), agents(), "portal".into(), portal).unwrap();
        crate::router(services().await, auth, mcp, vec![])
    }

    fn bearer(claims: Value) -> String {
        bearer_for("awzy-mcp", claims)
    }

    fn bearer_for(audience: &str, claims: Value) -> String {
        let mut claims = claims;
        claims.as_object_mut().unwrap().extend(
            json!({ "iss": "https://idp.test", "aud": audience, "exp": 4_000_000_000_u64 })
                .as_object()
                .unwrap()
                .clone(),
        );
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(b"test-secret"),
        )
        .unwrap();
        format!("Bearer {token}")
    }

    /// One JSON-RPC exchange with the /mcp endpoint, as the agent acting for alice. Returns the
    /// response body and session ID.
    async fn rpc(app: &Router, body: Value, session: Option<&str>) -> (Value, Option<String>) {
        let token = bearer(json!({ "sub": "alice", "groups": ["security"], "act": { "sub": "sec-agent" } }));
        let mut req = Request::post("/mcp")
            .header("authorization", token)
            .header("host", "localhost")
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18");
        if let Some(s) = session {
            req = req.header("mcp-session-id", s);
        }
        let res = app.clone().oneshot(req.body(Body::from(body.to_string())).unwrap()).await.unwrap();
        let status = res.status();
        let session = res.headers().get("mcp-session-id").map(|v| v.to_str().unwrap().to_string());
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(status.is_success(), "{status}: {text}");
        // JSON, or an SSE stream whose data lines carry the JSON-RPC message.
        let json = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or(text.trim());
        (if json.is_empty() { Value::Null } else { serde_json::from_str(json).unwrap() }, session)
    }

    #[tokio::test]
    async fn speaks_mcp_over_http_on_the_portal_router() {
        let app = app().await;

        let anonymous = app
            .clone()
            .oneshot(Request::post("/mcp").header("host", "localhost").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), 401);
        assert_eq!(
            anonymous.headers()["www-authenticate"],
            r#"Bearer error="invalid_token", resource_metadata="https://portal.test/.well-known/oauth-protected-resource/mcp""#
        );

        let (init, session) = rpc(
            &app,
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } } }),
            None,
        )
        .await;
        assert!(init["result"]["capabilities"]["tools"].is_object(), "{init}");
        rpc(&app, json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), session.as_deref()).await;

        let (tools, _) =
            rpc(&app, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }), session.as_deref()).await;
        let mut names: Vec<&str> =
            tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "check_access",
                "dry_run_policy",
                "explain_decision",
                "get_active_policy_bundle",
                "get_enforcement",
                "get_investigation",
                "get_principal",
                "get_resource",
                "list_investigations",
                "list_policy_bundles",
                "principal_activity",
                "propose_policy",
                "request_containment_lift",
                "search_decisions"
            ]
        );

        let (call, _) = rpc(
            &app,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                    "params": { "name": "propose_policy", "arguments": { "policies": awzy_pdp::DEFAULT_POLICIES } } }),
            session.as_deref(),
        )
        .await;
        assert_eq!(call["result"]["structuredContent"]["draft"]["status"], "draft", "{call}");
        assert_eq!(
            call["result"]["structuredContent"]["draft"]["created_by"], "ServiceAccount::sec-agent for Human::alice",
            "{call}"
        );
    }

    #[tokio::test]
    async fn a_token_for_mcp_doesnt_open_graphql_or_the_reverse() {
        let app = app().await;
        let alice = json!({ "sub": "alice", "groups": ["security"] });
        let status = |path: &'static str, token: String| {
            let app = app.clone();
            async move {
                let req = Request::post(path)
                    .header("authorization", token)
                    .header("host", "localhost")
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .body(Body::from(r#"{"query":"{ apiVersion }"}"#))
                    .unwrap();
                app.oneshot(req).await.unwrap().status()
            }
        };
        assert_eq!(status("/graphql", bearer_for("awzy-mcp", alice.clone())).await, 401, "an agent's MCP token");
        assert_eq!(status("/mcp", bearer_for("awzy", alice.clone())).await, 401, "the portal's own token");
        assert_eq!(status("/graphql", bearer_for("awzy", alice)).await, 200);

        let metadata = app
            .oneshot(Request::get("/.well-known/oauth-protected-resource/mcp").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(metadata.into_body(), usize::MAX).await.unwrap();
        let metadata: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(metadata["resource"], "https://portal.test/mcp");
        assert_eq!(metadata["authorization_servers"], json!(["https://idp.test"]));
    }
}
