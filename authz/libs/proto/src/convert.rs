//! Conversions between the wire types and `awzy-model`.

use awzy_model::{
    ChallengeKind as ModelChallenge, Challenges, Check, Decision, EntityRef as ModelRef, Verdict as ModelVerdict,
};

use awzy_model::EnforcementMode as ModelMode;

use crate::authz::v1::{DelegationCheck, EnforcementMode};

impl From<ModelMode> for EnforcementMode {
    fn from(m: ModelMode) -> Self {
        match m {
            ModelMode::Monitor => EnforcementMode::Monitor,
            ModelMode::Enforce => EnforcementMode::Enforce,
        }
    }
}

/// UNSPECIFIED reads as monitor: never enforce by accident.
impl From<EnforcementMode> for ModelMode {
    fn from(m: EnforcementMode) -> Self {
        match m {
            EnforcementMode::Enforce => ModelMode::Enforce,
            EnforcementMode::Monitor | EnforcementMode::Unspecified => ModelMode::Monitor,
        }
    }
}
use prost_types::{value::Kind, ListValue, Struct, Value};
use serde_json::{Map, Number, Value as Json};

use crate::authz::v1::{ChallengeKind, CheckRequest, CheckResponse, Entity, EntityRef, Verdict};

#[derive(Debug, thiserror::Error)]
pub enum ConvertError {
    #[error("{0} is required")]
    Missing(&'static str),
}

impl From<EntityRef> for ModelRef {
    fn from(r: EntityRef) -> Self {
        ModelRef::new(r.r#type, r.id)
    }
}

impl From<ModelRef> for EntityRef {
    fn from(r: ModelRef) -> Self {
        EntityRef { r#type: r.kind, id: r.id }
    }
}

impl From<awzy_model::Actor> for crate::authz::v1::Actor {
    fn from(a: awzy_model::Actor) -> Self {
        Self { subject: Some(a.subject.into()), on_behalf_of: a.on_behalf_of.map(Into::into) }
    }
}

/// Who acted, from a request's `…_actor` field, or its deprecated text field when the caller
/// predates ADR 0060. None when neither says.
pub fn actor_from_wire(actor: Option<crate::authz::v1::Actor>, text: &str) -> Option<awzy_model::Actor> {
    match actor.and_then(|a| Some((a.subject?, a.on_behalf_of))) {
        Some((subject, on_behalf_of)) => Some(awzy_model::Actor::new(subject.into(), on_behalf_of.map(Into::into))),
        None => (!text.is_empty()).then(|| awzy_model::Actor::parse(text)),
    }
}

/// The `…_actor` field and its deprecated text twin, for a response, from what's stored. Empty
/// stays empty: nobody has acted yet.
pub fn actor_to_wire(stored: &str) -> (Option<crate::authz::v1::Actor>, String) {
    if stored.is_empty() {
        return (None, String::new());
    }
    let actor = awzy_model::Actor::parse(stored);
    let text = actor.to_string();
    (Some(actor.into()), text)
}

impl TryFrom<CheckRequest> for Check {
    type Error = ConvertError;

    fn try_from(req: CheckRequest) -> Result<Self, Self::Error> {
        Ok(Check {
            principal: req.principal.ok_or(ConvertError::Missing("principal"))?.into(),
            action: req.action,
            resource: req.resource.ok_or(ConvertError::Missing("resource"))?.into(),
            context: req.context.map(struct_to_json).unwrap_or_default(),
            on_behalf_of: req.on_behalf_of.map(Into::into),
            human_present: req.human_present,
            always_enforced: req.always_enforced,
        })
    }
}

/// One Cedar entity JSON value (`uid`, `attrs`, `parents`) as a wire `Entity`.
pub fn entity_from_json(v: &Json) -> Entity {
    let r = |v: &Json| EntityRef {
        r#type: v["type"].as_str().unwrap_or_default().into(),
        id: v["id"].as_str().unwrap_or_default().into(),
    };
    Entity {
        uid: Some(r(&v["uid"])),
        attrs: v["attrs"].as_object().map(|o| json_to_struct(o.clone())),
        parents: v["parents"].as_array().map(|p| p.iter().map(r).collect()).unwrap_or_default(),
    }
}

/// Cedar entity JSON (`uid`, `attrs`, `parents`) for the inline entities of a request.
pub fn entities_to_json(entities: Vec<Entity>) -> Json {
    let uid = |r: EntityRef| serde_json::json!({ "type": r.r#type, "id": r.id });
    Json::Array(
        entities
            .into_iter()
            .map(|e| {
                serde_json::json!({
                    "uid": e.uid.map(uid).unwrap_or(Json::Null),
                    "attrs": Json::Object(e.attrs.map(struct_to_json).unwrap_or_default()),
                    "parents": e.parents.into_iter().map(uid).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn wire_verdict(v: ModelVerdict) -> (Verdict, Vec<i32>) {
    match v {
        ModelVerdict::Allow => (Verdict::Allow, vec![]),
        ModelVerdict::Deny => (Verdict::Deny, vec![]),
        ModelVerdict::Challenge { challenges } => (Verdict::Challenge, challenges_to_wire(challenges)),
    }
}

/// Challenge kinds as wire enum values, in [`Challenges::iter`] order.
pub fn challenges_to_wire(challenges: Challenges) -> Vec<i32> {
    challenges
        .iter()
        .map(|c| {
            match c {
                ModelChallenge::Justification => ChallengeKind::Justification,
                ModelChallenge::Verification => ChallengeKind::Verification,
                ModelChallenge::Approval => ChallengeKind::Approval,
                ModelChallenge::Biometric => ChallengeKind::Biometric,
            }
            .into()
        })
        .collect()
}

/// Wire enum values as challenge kinds. Unknown and unspecified values are dropped.
pub fn challenges_from_wire(kinds: &[i32]) -> Challenges {
    kinds
        .iter()
        .filter_map(|k| match ChallengeKind::try_from(*k).ok()? {
            ChallengeKind::Justification => Some(ModelChallenge::Justification),
            ChallengeKind::Verification => Some(ModelChallenge::Verification),
            ChallengeKind::Approval => Some(ModelChallenge::Approval),
            ChallengeKind::Biometric => Some(ModelChallenge::Biometric),
            ChallengeKind::Unspecified => None,
        })
        .collect()
}

impl From<Decision> for CheckResponse {
    fn from(d: Decision) -> Self {
        let (verdict, challenges) = wire_verdict(d.verdict);
        CheckResponse {
            decision_qid: d.id.to_string(),
            verdict: verdict.into(),
            challenges,
            policies: d.policies,
            on_behalf_of: d.on_behalf_of.map(|c| {
                let (verdict, challenges) = wire_verdict(c.verdict);
                DelegationCheck { verdict: verdict.into(), challenges, policies: c.policies }
            }),
            mode: EnforcementMode::from(d.mode).into(),
            challenge_qid: d.challenge_id.map(|id| id.to_string()).unwrap_or_default(),
        }
    }
}

pub fn struct_to_json(s: Struct) -> Map<String, Json> {
    s.fields.into_iter().map(|(k, v)| (k, value_to_json(v))).collect()
}

fn value_to_json(v: Value) -> Json {
    match v.kind {
        None | Some(Kind::NullValue(_)) => Json::Null,
        Some(Kind::BoolValue(b)) => Json::Bool(b),
        // protobuf has one number type. Whole numbers become JSON integers so Cedar sees Longs.
        Some(Kind::NumberValue(n)) if n.fract() == 0.0 && n.abs() < 9.0e15 => Json::Number(Number::from(n as i64)),
        Some(Kind::NumberValue(n)) => Number::from_f64(n).map(Json::Number).unwrap_or(Json::Null),
        Some(Kind::StringValue(s)) => Json::String(s),
        Some(Kind::ListValue(l)) => Json::Array(l.values.into_iter().map(value_to_json).collect()),
        Some(Kind::StructValue(s)) => Json::Object(struct_to_json(s)),
    }
}

pub fn json_to_struct(m: Map<String, Json>) -> Struct {
    Struct { fields: m.into_iter().map(|(k, v)| (k, json_to_value(v))).collect() }
}

fn json_to_value(v: Json) -> Value {
    let kind = match v {
        Json::Null => Kind::NullValue(0),
        Json::Bool(b) => Kind::BoolValue(b),
        Json::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or_default()),
        Json::String(s) => Kind::StringValue(s),
        Json::Array(a) => Kind::ListValue(ListValue { values: a.into_iter().map(json_to_value).collect() }),
        Json::Object(o) => Kind::StructValue(json_to_struct(o)),
    };
    Value { kind: Some(kind) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_round_trips_with_integers() {
        let ctx = json!({ "risk_score": 90, "ratio": 0.5, "tags": ["a"], "nested": { "ok": true } });
        let back = struct_to_json(json_to_struct(ctx.as_object().unwrap().clone()));
        assert_eq!(Json::Object(back), ctx);
    }

    #[test]
    fn challenge_kinds_round_trip() {
        let kinds: Challenges = [ModelChallenge::Approval, ModelChallenge::Justification].into_iter().collect();
        assert_eq!(challenges_from_wire(&challenges_to_wire(kinds)), kinds);
        assert!(challenges_from_wire(&[0, 99]).is_empty(), "unspecified and unknown values are dropped");
    }

    #[test]
    fn check_request_requires_principal() {
        let err = Check::try_from(CheckRequest::default()).unwrap_err();
        assert_eq!(err.to_string(), "principal is required");
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        /// JSON as enforcement points send context: integers well within f64's exact range,
        /// finite fractions, strings, and nesting.
        fn json() -> impl Strategy<Value = Json> {
            let leaf = prop_oneof![
                Just(Json::Null),
                any::<bool>().prop_map(Json::Bool),
                (-(1i64 << 52)..(1i64 << 52)).prop_map(Json::from),
                (-1.0e6f64..1.0e6).prop_filter("a fraction", |f| f.fract() != 0.0).prop_map(Json::from),
                ".*".prop_map(Json::String),
            ];
            leaf.prop_recursive(3, 32, 4, |inner| {
                prop_oneof![
                    prop::collection::vec(inner.clone(), 0..4).prop_map(Json::Array),
                    prop::collection::btree_map(".*", inner, 0..4).prop_map(|m| Json::Object(m.into_iter().collect())),
                ]
            })
        }

        proptest! {
            /// Context survives the trip through protobuf's Struct unchanged, integers included.
            #[test]
            fn context_round_trips(m in prop::collection::btree_map(".*", json(), 0..6)) {
                let m: Map<String, Json> = m.into_iter().collect();
                prop_assert_eq!(struct_to_json(json_to_struct(m.clone())), m);
            }
        }
    }
}
