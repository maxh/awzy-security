//! awzy-reasoner's investigations (ADR 0071): what a model found about a principal the reasoner
//! flagged, after awzy's checks decided what it may do (ADR 0067). Read from awzy-context; a
//! person closes one here once they've reviewed it.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject, ID};
use awzy_model::InvestigationId;
use awzy_portal::{Actor, Qid, UtcTimestamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    access::{require, PortalAction, PortalArea},
    decisions::EntityRef,
    directory::{context, ContextClient},
};

/// The most one list returns.
const MOST: i32 = 200;

// ---- What awzy-context sends

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WireSubject {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WireCited {
    pub id: String,
    #[serde(default)]
    pub why: String,
}

/// One investigation as awzy-context keeps it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WireInvestigation {
    pub qid: String,
    pub principal: WireSubject,
    pub trigger: String,
    pub floor: i32,
    pub status: String,
    pub verdict: String,
    pub risk: i32,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<WireCited>,
    pub containment: String,
    #[serde(default)]
    pub notes: Vec<String>,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub closed_by: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_reason: Option<String>,
}

// ---- Operations, shared with the MCP tools

/// Newest first, optionally only those with `status`.
pub async fn list(context: &ContextClient, status: Option<&str>, first: i32) -> anyhow::Result<Vec<WireInvestigation>> {
    #[derive(Deserialize)]
    struct Body {
        investigations: Vec<WireInvestigation>,
    }
    let status = status.map(|s| format!("&status={s}")).unwrap_or_default();
    let body: Body = context.get(&format!("/v1/investigations?limit={}{status}", first.clamp(1, MOST))).await?;
    Ok(body.investigations)
}

/// One investigation, or None when there's no such one.
pub async fn get(context: &ContextClient, qid: &str) -> anyhow::Result<Option<WireInvestigation>> {
    #[derive(Deserialize)]
    struct Body {
        investigation: WireInvestigation,
    }
    let (status, body) =
        context.call("GET", &format!("/v1/investigations/{}", awzy_http::encode(qid)), None, None).await?;
    match status {
        200 => Ok(Some(serde_json::from_slice::<Body>(&body)?.investigation)),
        400 | 404 => Ok(None),
        _ => anyhow::bail!("awzy-context answered {status}: {}", String::from_utf8_lossy(&body)),
    }
}

/// Closes `qid` as `actor`, for `reason`.
pub async fn close(
    context: &ContextClient,
    qid: &str,
    reason: &str,
    actor: &awzy_model::Actor,
) -> std::result::Result<WireInvestigation, String> {
    #[derive(Deserialize)]
    struct Body {
        investigation: WireInvestigation,
    }
    let path = format!("/v1/investigations/{}/close", awzy_http::encode(qid));
    let (status, body) = context
        .call("POST", &path, Some(&actor.to_string()), Some(json!({ "reason": reason })))
        .await
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&body).to_string();
    match status {
        200 => serde_json::from_slice::<Body>(&body).map(|b| b.investigation).map_err(|e| e.to_string()),
        400 | 404 | 409 => Err(text),
        _ => Err(format!("awzy-context answered {status}: {text}")),
    }
}

// ---- GraphQL

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum InvestigationStatus {
    /// A person has to decide: a proposed containment, or benign about someone the scores flag.
    NeedsReview,
    /// Nothing for a person to do.
    Done,
    /// A person reviewed and closed it.
    Closed,
}

impl InvestigationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::NeedsReview => "needs_review",
            Self::Done => "done",
            Self::Closed => "closed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "needs_review" => Self::NeedsReview,
            "closed" => Self::Closed,
            _ => Self::Done,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum InvestigationVerdict {
    Malicious,
    Suspicious,
    Benign,
    Inconclusive,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum InvestigationContainment {
    /// None was recommended, or awzy's checks dropped it.
    None,
    /// For a person to decide: the principal is privileged.
    Propose,
    /// Sent to automated containments, which apply it only when enforcing.
    Recommend,
}

/// One piece of evidence the finding relied on.
#[derive(SimpleObject)]
pub struct CitedEvidence {
    /// The evidence's ID: a decision or challenge QID, an event's ID, or a resource ID.
    pub evidence_id: String,
    /// What the model said it shows.
    pub why: String,
}

/// One investigation.
pub struct Investigation(pub WireInvestigation);

#[Object]
impl Investigation {
    /// QID: investigation_<uuid>.
    pub async fn id(&self) -> ID {
        ID(self.0.qid.clone())
    }

    /// The person or service account it's about.
    async fn principal(&self) -> EntityRef {
        EntityRef { kind: self.0.principal.kind.clone(), id: self.0.principal.id.clone() }
    }

    /// Why the reasoner opened it.
    async fn trigger(&self) -> &str {
        &self.0.trigger
    }

    /// The deterministic risk score when it ran: the floor for its risk.
    async fn floor(&self) -> i32 {
        self.0.floor
    }

    async fn status(&self) -> InvestigationStatus {
        InvestigationStatus::parse(&self.0.status)
    }

    async fn verdict(&self) -> InvestigationVerdict {
        match self.0.verdict.as_str() {
            "malicious" => InvestigationVerdict::Malicious,
            "suspicious" => InvestigationVerdict::Suspicious,
            "benign" => InvestigationVerdict::Benign,
            _ => InvestigationVerdict::Inconclusive,
        }
    }

    async fn risk(&self) -> i32 {
        self.0.risk
    }

    async fn summary(&self) -> &str {
        &self.0.summary
    }

    async fn evidence(&self) -> Vec<CitedEvidence> {
        self.0.evidence.iter().map(|c| CitedEvidence { evidence_id: c.id.clone(), why: c.why.clone() }).collect()
    }

    async fn containment(&self) -> InvestigationContainment {
        match self.0.containment.as_str() {
            "propose" => InvestigationContainment::Propose,
            "recommend" => InvestigationContainment::Recommend,
            _ => InvestigationContainment::None,
        }
    }

    /// What awzy's checks changed in the model's finding.
    async fn notes(&self) -> &[String] {
        &self.0.notes
    }

    async fn model(&self) -> &str {
        &self.0.model
    }

    async fn input_tokens(&self) -> i64 {
        self.0.input_tokens
    }

    async fn output_tokens(&self) -> i64 {
        self.0.output_tokens
    }

    async fn started_at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.started_at)
    }

    async fn finished_at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.finished_at)
    }

    async fn closed_by(&self) -> Option<Actor> {
        self.0.closed_by.as_deref().and_then(Actor::from_text)
    }

    async fn closed_at(&self) -> Option<UtcTimestamp> {
        self.0.closed_at.map(UtcTimestamp)
    }

    async fn close_reason(&self) -> Option<&str> {
        self.0.close_reason.as_deref()
    }
}

#[derive(Default)]
pub struct InvestigationQuery;

#[Object]
impl InvestigationQuery {
    /// Investigations, newest first, optionally only those with one status.
    async fn investigations(
        &self,
        ctx: &Context<'_>,
        status: Option<InvestigationStatus>,
        #[graphql(default = 50)] first: i32,
    ) -> Result<Vec<Investigation>> {
        require(ctx, PortalAction::View, PortalArea::Investigations).await?;
        let list = list(context(ctx)?, status.map(InvestigationStatus::as_str), first).await?;
        Ok(list.into_iter().map(Investigation).collect())
    }
}

/// One investigation, for `node(id)`. For those who may view investigations.
pub(crate) async fn get_by_id(ctx: &Context<'_>, id: InvestigationId) -> Result<Option<Investigation>> {
    require(ctx, PortalAction::View, PortalArea::Investigations).await?;
    Ok(get(context(ctx)?, &id.to_string()).await?.map(Investigation))
}

#[derive(InputObject)]
pub struct CloseInvestigationInput {
    pub investigation_qid: Qid<InvestigationId>,
    /// What the review found.
    pub reason: String,
}

#[derive(SimpleObject)]
pub struct CloseInvestigationPayload {
    pub investigation: Investigation,
}

#[derive(Default)]
pub struct InvestigationMutation;

#[Object]
impl InvestigationMutation {
    /// Closes an investigation once a person has reviewed it. It stays, with who closed it and why.
    async fn close_investigation(
        &self,
        ctx: &Context<'_>,
        input: CloseInvestigationInput,
    ) -> Result<CloseInvestigationPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Investigations).await?;
        let reason = input.reason.trim();
        if reason.is_empty() {
            return Err("say what the review found".into());
        }
        let closed = close(context(ctx)?, &input.investigation_qid.to_string(), reason, &viewer.actor()).await?;
        Ok(CloseInvestigationPayload { investigation: Investigation(closed) })
    }
}

/// Plain values for the MCP tools: what an agent needs to read one.
pub fn to_json(i: &WireInvestigation) -> Value {
    json!({
        "investigation_qid": i.qid,
        "principal": { "type": i.principal.kind, "id": i.principal.id },
        "trigger": i.trigger,
        "floor": i.floor,
        "status": i.status,
        "verdict": i.verdict,
        "risk": i.risk,
        "summary": i.summary,
        "evidence": i.evidence,
        "containment": i.containment,
        "notes": i.notes,
        "finished_at": i.finished_at.to_rfc3339(),
        "closed_by": i.closed_by,
        "close_reason": i.close_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_round_trip_and_unknown_ones_are_done() {
        for s in [InvestigationStatus::NeedsReview, InvestigationStatus::Done, InvestigationStatus::Closed] {
            assert!(InvestigationStatus::parse(s.as_str()) == s);
        }
        assert!(InvestigationStatus::parse("something new") == InvestigationStatus::Done);
    }

    #[test]
    fn what_awzy_context_sends_reads_as_an_investigation() {
        let wire: WireInvestigation = serde_json::from_value(json!({
            "qid": "investigation_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90",
            "principal": { "type": "Human", "id": "nora" },
            "trigger": "risk 63", "floor": 63, "status": "needs_review", "verdict": "suspicious", "risk": 70,
            "summary": "s", "evidence": [{ "id": "decision_1", "why": "the export" }], "containment": "none",
            "notes": [], "finding": null, "model": "m", "input_tokens": 10, "output_tokens": 2,
            "started_at": "2026-10-10T15:30:00Z", "finished_at": "2026-10-10T15:31:00Z",
            "closed_by": null, "closed_at": null, "close_reason": null,
        }))
        .unwrap();
        let out = to_json(&wire);
        assert_eq!(out["investigation_qid"], wire.qid);
        assert_eq!(out["evidence"][0]["why"], "the export");
        assert_eq!(awzy_http::encode("investigation_0199/x"), "investigation_0199%2Fx");
    }
}
