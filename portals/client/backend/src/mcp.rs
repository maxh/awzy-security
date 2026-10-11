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
    tool, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData,
};

use crate::{
    access::{self, PortalAction, PortalArea, Surface, Viewer},
    decisions::{self, DecisionRow, SearchParams},
    policies::{self, DraftOutcome},
    Services,
};

#[cfg(test)]
mod tests;
mod types;

pub use types::*;

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
            context: self
                .visible(&viewer, PortalArea::Decisions, "Decision.context")
                .await?
                .then(|| row.context.clone()),
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
        let mut out = principal_from_snapshot(&snapshot, &p.principal_type, &p.principal_id);
        // The fields a policy may hide (ADR 0103), as the portal's PrincipalDetail does.
        if !self.visible(&viewer, PortalArea::Directory, "PrincipalDetail.attributes").await? {
            if let Some(entity) = out.entity.as_mut().and_then(|e| e.as_object_mut()) {
                entity.remove("attrs");
            }
        }
        if !self.visible(&viewer, PortalArea::Directory, "PrincipalDetail.riskScore").await? {
            out.risk_score = None;
        }
        Ok(Json(out))
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
        let mut entities = found["entities"].as_array().cloned().unwrap_or_default();
        // Who a resource and its folders are shared with, unless a policy hides it (ADR 0103).
        if !self.visible(&viewer, PortalArea::Directory, "ResourceDetail.sharing").await? {
            for e in &mut entities {
                if let Some(attrs) = e["attrs"].as_object_mut() {
                    attrs.retain(|k, _| !k.starts_with("shared_"));
                }
            }
        }
        Ok(Json(ResourceOut { entities, assignees: strings(&found["assignees"]) }))
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
    /// Whether the viewer may see one of the sensitive fields (ADR 0103), as the portal checks it.
    async fn visible(&self, viewer: &Viewer, area: PortalArea, field: &str) -> Result<bool, ErrorData> {
        access::field_visible(&self.services, viewer, Surface::Mcp, area, field)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }

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
pub(crate) fn principal_from_snapshot(snapshot: &serde_json::Value, kind: &str, id: &str) -> PrincipalOut {
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
