//! Shared domain types: access requests, verdicts, and decisions.
//!
//! Every service and both portals agree on these shapes at compile time.

mod actor;
mod qid;

pub use actor::{actor_type, Actor};
pub use qid::{
    CeremonyId, ChallengeId, ContainmentId, DecisionId, EventId, InstallId, InvestigationId, PolicyBundleId, QidError,
    SecurityKeyId,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Entity type names the PDP and the portals depend on.
pub mod entity_type {
    pub const HUMAN: &str = "Human";
    pub const SERVICE_ACCOUNT: &str = "ServiceAccount";
    pub const GROUP: &str = "Group";
    pub const RESOURCE: &str = "Resource";
    pub const MCP_SERVER: &str = "McpServer";
    pub const MCP_TOOL: &str = "McpTool";
    pub const PORTAL_AREA: &str = "PortalArea";
}

/// A typed reference to a world-model entity, e.g. `Human::"alice"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
}

impl EntityRef {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self { kind: kind.into(), id: id.into() }
    }
}

/// One action by one accessor on one resource: the unit Beyond Zero authorizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessRequest {
    pub subject: EntityRef,
    pub action: String,
    pub resource: EntityRef,
    #[serde(default)]
    pub context: Map<String, Value>,
    /// When a service account acts for a person (OAuth token exchange, RFC 8693), that
    /// person. The PDP allows a delegated request only if the person could do it too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_behalf_of: Option<EntityRef>,
    /// Whether a person is there to answer a challenge. Unset means: yes for a Human
    /// subject, no for a service account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human_present: Option<bool>,
    /// The enforcement point applies the verdict whatever the install's mode (ADR 0020), so the
    /// decision is recorded as enforced.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always_enforced: bool,
}

impl AccessRequest {
    pub fn human_present(&self) -> bool {
        self.human_present.unwrap_or(self.subject.kind == entity_type::HUMAN)
    }
}

/// The kinds of challenge awzy-challenge can issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeKind {
    Justification,
    Verification,
    Approval,
    Biometric,
}

impl ChallengeKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "justification" => Some(Self::Justification),
            "verification" => Some(Self::Verification),
            "approval" => Some(Self::Approval),
            "biometric" => Some(Self::Biometric),
            _ => None,
        }
    }
}

impl ChallengeKind {
    const ALL: [ChallengeKind; 4] = [Self::Justification, Self::Verification, Self::Approval, Self::Biometric];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Justification => "justification",
            Self::Verification => "verification",
            Self::Approval => "approval",
            Self::Biometric => "biometric",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }
}

/// The challenges a verdict requires. Every one must be passed: when several `@challenge`
/// policies fire, or both checks of a delegated request challenge, their kinds are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Challenges(u8);

impl Challenges {
    pub fn of(kind: ChallengeKind) -> Self {
        Self(kind.bit())
    }

    pub fn with(self, kind: ChallengeKind) -> Self {
        Self(self.0 | kind.bit())
    }

    pub fn union(self, other: Challenges) -> Self {
        Self(self.0 | other.0)
    }

    /// The kinds in both.
    pub fn intersection(self, other: Challenges) -> Self {
        Self(self.0 & other.0)
    }

    /// These kinds less `other`'s: what is still to pass once `other` is passed.
    pub fn without(self, other: Challenges) -> Self {
        Self(self.0 & !other.0)
    }

    pub fn contains(self, kind: ChallengeKind) -> bool {
        self.0 & kind.bit() != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// In a fixed order: justification, verification, approval, biometric.
    pub fn iter(self) -> impl Iterator<Item = ChallengeKind> {
        ChallengeKind::ALL.into_iter().filter(move |k| self.0 & k.bit() != 0)
    }

    /// `approval,verification`: how the decision log stores them.
    pub fn joined(self) -> String {
        self.iter().map(ChallengeKind::as_str).collect::<Vec<_>>().join(",")
    }
}

impl FromIterator<ChallengeKind> for Challenges {
    fn from_iter<I: IntoIterator<Item = ChallengeKind>>(iter: I) -> Self {
        iter.into_iter().fold(Self::default(), Self::with)
    }
}

impl Serialize for Challenges {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> Deserialize<'de> for Challenges {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Vec::<ChallengeKind>::deserialize(d)?.into_iter().collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    Allow,
    Deny,
    /// Allowed only after every listed challenge is passed.
    Challenge {
        challenges: Challenges,
    },
}

/// Whether enforcement points apply verdicts (ADR 0007). In monitor, the PDP decides and the
/// decision is recorded, but the enforcement point allows. Every new scope starts in monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    #[default]
    Monitor,
    Enforce,
}

/// The enforcement scopes awzy-policy keeps a mode for: the install's verdicts, and
/// awzy-reasoner's automated containments (ADR 0066).
pub const INSTALL_SCOPE: &str = "install";
pub const AUTOMATED_CONTAINMENTS_SCOPE: &str = "automated-containments";

impl EnforcementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monitor => "monitor",
            Self::Enforce => "enforce",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "monitor" => Some(Self::Monitor),
            "enforce" => Some(Self::Enforce),
            _ => None,
        }
    }
}

/// A verdict plus the policies that produced it, for explain-decision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Time-ordered, so the decision log sorts by it. Used to explain the decision later.
    /// On the wire: `qid` (ADR 0058).
    #[serde(rename = "qid")]
    pub id: DecisionId,
    /// For a delegated request, the stricter of the subject's and the person's checks.
    #[serde(flatten)]
    pub verdict: Verdict,
    /// Policies that determined the subject's own check.
    pub policies: Vec<String>,
    /// For a delegated request: the check of the person the subject acted for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_behalf_of: Option<DelegationCheck>,
    /// In monitor the enforcement point must allow; `verdict` is what would have happened.
    #[serde(default)]
    pub mode: EnforcementMode,
    /// For an enforced challenge: the challenge awzy-challenge opened, which the person answers
    /// before the request is retried.
    #[serde(default, rename = "challenge_qid", skip_serializing_if = "Option::is_none")]
    pub challenge_id: Option<ChallengeId>,
}

/// The second check a delegated request gets: could the person do this themselves?
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelegationCheck {
    #[serde(flatten)]
    pub verdict: Verdict,
    pub policies: Vec<String>,
}

impl Verdict {
    /// Deny beats challenge beats allow; two challenges require both sets.
    pub fn stricter(self, other: Verdict) -> Verdict {
        match (self, other) {
            (Verdict::Deny, _) | (_, Verdict::Deny) => Verdict::Deny,
            (Verdict::Challenge { challenges: a }, Verdict::Challenge { challenges: b }) => {
                Verdict::Challenge { challenges: a.union(b) }
            }
            (Verdict::Challenge { .. }, _) => self,
            (_, Verdict::Challenge { .. }) => other,
            _ => Verdict::Allow,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Allow => "allow",
            Verdict::Deny => "deny",
            Verdict::Challenge { .. } => "challenge",
        }
    }
}

/// One row of the decision log: what was asked, what was decided, and under which policy
/// bundle. Serialized as JSON onto the decisions topic and read by ClickHouse's Kafka engine,
/// so field names here are the ClickHouse column names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub id: DecisionId,
    /// On the wire: `at_ms`, milliseconds since the Unix epoch.
    #[serde(rename = "at_ms", with = "chrono::serde::ts_milliseconds")]
    pub at: DateTime<Utc>,
    pub subject_type: String,
    pub subject_id: String,
    /// The person a service account acted for; empty when not delegated.
    pub on_behalf_of_type: String,
    pub on_behalf_of_id: String,
    pub human_present: bool,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    /// allow | deny | challenge: what policy decided.
    pub verdict: String,
    /// Whether the verdict was applied. In monitor, a deny is a "would deny".
    pub mode: EnforcementMode,
    /// The challenge kind when verdict is challenge, otherwise empty.
    pub challenge: String,
    pub policies: Vec<String>,
    /// For a delegated request, the person's own check: allow | deny | challenge. Empty otherwise.
    pub on_behalf_of_verdict: String,
    pub on_behalf_of_policies: Vec<String>,
    pub policy_version: i64,
    /// The request context as JSON text.
    pub context: String,
}

impl DecisionRecord {
    pub fn new(req: &AccessRequest, decision: &Decision, policy_version: i64, at: DateTime<Utc>) -> Self {
        let challenge = match decision.verdict {
            Verdict::Challenge { challenges } => challenges.joined(),
            _ => String::new(),
        };
        let (on_behalf_of_type, on_behalf_of_id) =
            req.on_behalf_of.as_ref().map(|h| (h.kind.clone(), h.id.clone())).unwrap_or_default();
        let (on_behalf_of_verdict, on_behalf_of_policies) = decision
            .on_behalf_of
            .as_ref()
            .map(|d| (d.verdict.as_str().to_string(), d.policies.clone()))
            .unwrap_or_default();
        Self {
            id: decision.id,
            at,
            subject_type: req.subject.kind.clone(),
            subject_id: req.subject.id.clone(),
            on_behalf_of_type,
            on_behalf_of_id,
            human_present: req.human_present(),
            action: req.action.clone(),
            resource_type: req.resource.kind.clone(),
            resource_id: req.resource.id.clone(),
            verdict: decision.verdict.as_str().to_string(),
            mode: decision.mode,
            challenge,
            policies: decision.policies.clone(),
            on_behalf_of_verdict,
            on_behalf_of_policies,
            policy_version,
            context: Value::Object(req.context.clone()).to_string(),
        }
    }
}

/// One thing an accessor did, as an app or audit log recorded it rather than as a decision:
/// awzy-intake normalizes each source into this and sends it to Kafka (`awzy.events.v1`), where
/// ClickHouse picks it up. Empty strings mean the source didn't say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: EventId,
    /// When it happened, per the source. On the wire: `at_ms`.
    #[serde(rename = "at_ms", with = "chrono::serde::ts_milliseconds")]
    pub at: DateTime<Utc>,
    /// When awzy-intake took it in. On the wire: `received_at_ms`.
    #[serde(rename = "received_at_ms", with = "chrono::serde::ts_milliseconds")]
    pub received_at: DateTime<Utc>,
    /// The format it arrived in, which names its normalizer: `app` or `aws_cloudtrail`.
    pub source: String,
    /// The system that logged it: an app's name, or an AWS service such as `s3.amazonaws.com`.
    pub system: String,
    pub subject_type: String,
    pub subject_id: String,
    /// The person a service account acted for, when the source records it.
    pub on_behalf_of_type: String,
    pub on_behalf_of_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    /// success | failure
    pub outcome: String,
    pub source_ip: String,
    /// Source-specific fields, as JSON text.
    pub attributes: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_serializes_flat() {
        let id = DecisionId::new();
        let d = Decision {
            id,
            verdict: Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) },
            policies: vec!["high-risk-verify".into()],
            on_behalf_of: None,
            mode: EnforcementMode::Enforce,
            challenge_id: None,
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "qid": id.to_string(),
                "verdict": "challenge",
                "challenges": ["verification"],
                "policies": ["high-risk-verify"],
                "mode": "enforce",
            })
        );
    }

    #[test]
    fn decision_record_flattens_request_and_verdict() {
        let req = AccessRequest {
            subject: EntityRef::new("ServiceAccount", "sales-genie"),
            action: "read".into(),
            resource: EntityRef::new("Resource", "plan"),
            context: serde_json::json!({ "risk_score": 90 }).as_object().unwrap().clone(),
            on_behalf_of: Some(EntityRef::new("Human", "bob")),
            human_present: None,
            always_enforced: false,
        };
        let d = Decision {
            id: DecisionId::new(),
            verdict: Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) },
            policies: vec!["p".into()],
            on_behalf_of: Some(DelegationCheck { verdict: Verdict::Allow, policies: vec!["h".into()] }),
            mode: EnforcementMode::Monitor,
            challenge_id: None,
        };
        let at = DateTime::from_timestamp_millis(1_700_000_000_123).unwrap();
        let r = DecisionRecord::new(&req, &d, 4, at);
        let wire = serde_json::to_value(&r).unwrap();
        assert_eq!(wire["at_ms"], 1_700_000_000_123_i64);
        assert_eq!(wire["id"], d.id.to_string());
        assert_eq!(wire["mode"], "monitor");
        assert_eq!((r.verdict.as_str(), r.challenge.as_str()), ("challenge", "approval"));
        assert_eq!((r.on_behalf_of_type.as_str(), r.on_behalf_of_id.as_str()), ("Human", "bob"));
        assert_eq!(
            (r.on_behalf_of_verdict.as_str(), r.on_behalf_of_policies.as_slice()),
            ("allow", &["h".to_string()][..])
        );
        assert!(!r.human_present, "a service account is not a person present");
        assert_eq!(r.context, r#"{"risk_score":90}"#);
        assert_eq!(r.policy_version, 4);
    }

    #[test]
    fn stricter_verdict_wins() {
        let ch = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) };
        let key = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };
        assert_eq!(Verdict::Allow.stricter(Verdict::Allow), Verdict::Allow);
        assert_eq!(Verdict::Allow.stricter(ch), ch);
        assert_eq!(ch.stricter(Verdict::Deny), Verdict::Deny);
        assert_eq!(Verdict::Deny.stricter(ch), Verdict::Deny);
        let both = ch.stricter(key);
        assert_eq!(
            both,
            Verdict::Challenge {
                challenges: [ChallengeKind::Approval, ChallengeKind::Verification].into_iter().collect()
            }
        );
        let Verdict::Challenge { challenges } = both else { unreachable!() };
        assert_eq!(challenges.joined(), "verification,approval");
        assert_eq!(serde_json::to_value(both).unwrap()["challenges"], serde_json::json!(["verification", "approval"]));
    }

    #[test]
    fn challenge_kind_parses() {
        assert_eq!(ChallengeKind::parse("approval"), Some(ChallengeKind::Approval));
        assert_eq!(ChallengeKind::parse("nope"), None);
    }

    #[test]
    fn passing_some_challenges_leaves_the_rest() {
        let required: Challenges = [ChallengeKind::Justification, ChallengeKind::Approval].into_iter().collect();
        let passed: Challenges = [ChallengeKind::Approval, ChallengeKind::Verification].into_iter().collect();
        assert_eq!(required.without(passed), Challenges::of(ChallengeKind::Justification));
        assert_eq!(required.intersection(passed), Challenges::of(ChallengeKind::Approval));
        assert!(required.contains(ChallengeKind::Approval) && !required.contains(ChallengeKind::Biometric));
        assert!(required.without(required).is_empty());
    }
}
