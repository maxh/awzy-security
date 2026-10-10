//! MCP server for the client's own AI agents, served at `/mcp` (streamable HTTP).
//!
//! Lets a security team's agent explain decisions, search the decision log, read policy
//! bundles, and propose a policy change. Each tool calls the same operations as the GraphQL
//! resolvers, so a tool can't see or do more than the matching portal action. Tools that
//! would change enforcement only create drafts: a human publishes them in the portal.
//!
//! Callers sign in like portal users (crate::access): usually an agent with a token-exchange
//! token, checked as the service account acting for its person, with `context.client` "mcp".
//! TODO: record each call with awzy-intake.

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
    /// Only decisions whose subject (user or agent) has this ID, e.g. "alice".
    pub subject_id: Option<String>,
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
    pub subject: String,
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
    /// IDs of the policies that determined the subject's own check.
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
    /// Policies that determined the subject's own check.
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
    /// Each with its subject, verdict, risk, summary, cited evidence, containment, and status.
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
        description = "awzy-reasoner's investigations of accessors it flagged, newest first: what the model found, the evidence it cited, and what awzy's checks let it do. Filter by status: needs_review, done, or closed. Read-only."
    )]
    async fn list_investigations(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<ListInvestigationsParams>,
    ) -> Result<Json<InvestigationList>, ErrorData> {
        self.allow(&parts, PortalAction::View, PortalArea::Investigations).await?;
        if let Some(s) = p.status.as_deref().filter(|s| !["needs_review", "done", "closed"].contains(s)) {
            return Err(ErrorData::invalid_params(format!("status is needs_review, done, or closed, not {s:?}"), None));
        }
        let limit = p.limit.unwrap_or(20).min(200) as i32;
        let list = crate::investigations::list(self.context()?, p.status.as_deref(), limit).await.map_err(internal)?;
        Ok(Json(InvestigationList { investigations: list.iter().map(crate::investigations::to_json).collect() }))
    }

    #[tool(
        name = "get_investigation",
        description = "One investigation by its QID: its subject, why it was opened, the verdict, risk, summary, the evidence it cited, what awzy's checks changed, and whether a person has closed it. Read-only."
    )]
    async fn get_investigation(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<GetInvestigationParams>,
    ) -> Result<Json<InvestigationOut>, ErrorData> {
        self.allow(&parts, PortalAction::View, PortalArea::Investigations).await?;
        let found = crate::investigations::get(self.context()?, &p.investigation_qid).await.map_err(internal)?;
        Ok(Json(InvestigationOut { investigation: found.as_ref().map(crate::investigations::to_json) }))
    }

    #[tool(
        name = "search_decisions",
        description = "Search authorization decisions, newest first. Filter by subject, resource, verdict, and time (last 24 hours by default, at most 30 days)."
    )]
    async fn search_decisions(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(p): Parameters<SearchDecisionsParams>,
    ) -> Result<Json<DecisionList>, ErrorData> {
        self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        let since = p.since.as_deref().map(parse_time).transpose()?;
        let page = decisions::search(
            self.clickhouse()?,
            SearchParams {
                subject_id: p.subject_id,
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
        self.allow(&parts, PortalAction::View, PortalArea::Decisions).await?;
        let id: DecisionId =
            p.decision_qid.parse().map_err(|e: awzy_model::QidError| ErrorData::invalid_params(e.to_string(), None))?;
        let row = decisions::find(self.clickhouse()?, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| ErrorData::invalid_params(format!("{id} is not in the decision log"), None))?;
        let policy = self.services.policy.clone();
        let own = decisions::explain(policy.clone(), row.policy_version, &row.policies).await.map_err(internal)?;
        let person =
            decisions::explain(policy, row.policy_version, &row.on_behalf_of_policies).await.map_err(internal)?;
        Ok(Json(Explanation {
            decision: summary_of(&row),
            context: row.context.clone(),
            summary: explain_sentence(&row, &own),
            determining_policies: own.into_iter().map(|p| PolicySourceOut { id: p.id, source: p.source }).collect(),
            on_behalf_of_policies: person.into_iter().map(|p| PolicySourceOut { id: p.id, source: p.source }).collect(),
        }))
    }

    #[tool(
        name = "get_active_policy_bundle",
        description = "The Cedar policies and schema the PDPs currently enforce."
    )]
    async fn get_active_policy_bundle(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<Json<ActiveBundle>, ErrorData> {
        self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
        let bundle = policies::active(self.policy()?).await.map_err(internal)?;
        Ok(Json(ActiveBundle { bundle: bundle.map(|b| bundle_out(b, true)) }))
    }

    #[tool(
        name = "get_enforcement",
        description = "Whether awzy is enforcing verdicts or only monitoring them, and over the last 24 hours how many decisions would have been denied or challenged. Read-only: switching modes is done by a person in the client portal."
    )]
    async fn get_enforcement(&self, Extension(parts): Extension<Parts>) -> Result<Json<EnforcementOut>, ErrorData> {
        self.allow(&parts, PortalAction::View, PortalArea::Enforcement).await?;
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
        self.allow(&parts, PortalAction::View, PortalArea::Policies).await?;
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
    let person = format!("{}::{}", row.on_behalf_of_type, row.on_behalf_of_id);
    if !row.on_behalf_of_id.is_empty() && row.on_behalf_of_verdict == "deny" && row.verdict == "deny" {
        let subject_alone = if row.policies.is_empty() {
            "and the service account couldn't either"
        } else {
            "even though the service account could"
        };
        return format!("Denied: {person} couldn't do this themselves, {subject_alone}. A service account acting for someone gets at most what they could do.");
    }
    match (row.verdict.as_str(), own.is_empty()) {
        ("deny", true) => "Denied by default: no policy permitted this action.".to_string(),
        ("allow", _) if row.on_behalf_of_id.is_empty() => format!("Allowed by {}.", ids(own)),
        ("allow", _) => format!("Allowed by {}, and {person} could do it themselves.", ids(own)),
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
        subject: format!("{}::{}", row.subject_type, row.subject_id),
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
        signed_in(testing::person("alice", "security"))
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

        let bob = signed_in(testing::person("bob", "engineering"));
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
            subject_type: "ServiceAccount".into(),
            subject_id: "ops-bot".into(),
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

    /// Portal auth with an HS256 test key, and a token for `sub` acting for `person`.
    fn auth() -> access::Auth {
        std::env::set_var(SECRET_ENV, "test-secret");
        let token = awzy_identity::TokenConfig {
            issuer: "https://idp.test".into(),
            audience: "awzy".into(),
            jwks_ca_file: None,
            subject_claim: None,
            key: awzy_identity::TokenKey::Hs256SecretEnv(SECRET_ENV.into()),
        };
        let agent = awzy_identity::ServiceAccountConfig {
            id: "sec-agent".into(),
            owner: "alice".into(),
            ai: true,
            groups: vec!["security".into()],
        };
        access::Auth::new(token, vec![agent], "portal".into(), Some("https://portal.test".into())).unwrap()
    }

    fn bearer(claims: Value) -> String {
        let mut claims = claims;
        claims.as_object_mut().unwrap().extend(
            json!({ "iss": "https://idp.test", "aud": "awzy", "exp": 4_000_000_000_u64 }).as_object().unwrap().clone(),
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
        let app = crate::router(services().await, auth(), vec![]);

        let anonymous = app
            .clone()
            .oneshot(Request::post("/mcp").header("host", "localhost").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(anonymous.status(), 401);
        assert_eq!(
            anonymous.headers()["www-authenticate"],
            r#"Bearer error="invalid_token", resource_metadata="https://portal.test/.well-known/oauth-protected-resource""#
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
                "explain_decision",
                "get_active_policy_bundle",
                "get_enforcement",
                "get_investigation",
                "list_investigations",
                "list_policy_bundles",
                "propose_policy",
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
}
