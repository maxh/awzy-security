//! The `Qid` scalar: one of awzy's typed IDs on the wire (ADR 0013, 0100).
//!
//! Every entity's QIDs share the one GraphQL scalar, but in Rust a field or argument is
//! `Qid<ChallengeId>`, `Qid<InstallId>`, and so on, so a resolver gets the entity it expects and a
//! QID of any other entity is refused before it runs.

use std::{fmt, ops::Deref};

use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use awzy_model::TypedQid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Qid<T>(pub T);

impl<T> Deref for Qid<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: TypedQid> From<T> for Qid<T> {
    fn from(id: T) -> Self {
        Self(id)
    }
}

impl<T: TypedQid> fmt::Display for Qid<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One of awzy's IDs, `<entity_type>_<uuid>`, e.g. `challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90`.
/// Each field and argument takes one entity's; any other is refused.
#[Scalar(name = "Qid")]
impl<T: TypedQid> ScalarType for Qid<T> {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => s.parse::<T>().map(Qid).map_err(|e| InputValueError::custom(e.to_string())),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use awzy_model::{ChallengeId, InstallId};

    use super::*;

    #[test]
    fn a_qid_of_another_entity_is_refused() {
        let id = ChallengeId::new();
        let parsed = Qid::<ChallengeId>::parse(Value::String(id.to_string())).unwrap();
        assert_eq!(parsed.0, id);
        assert_eq!(parsed.to_value(), Value::String(id.to_string()));
        let install = InstallId::new().to_string();
        assert!(Qid::<ChallengeId>::parse(Value::String(install)).is_err(), "an install's QID isn't a challenge's");
        assert!(Qid::<ChallengeId>::parse(Value::Number(1.into())).is_err());
    }
}
