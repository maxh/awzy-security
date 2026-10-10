//! GraphQL scalars shared by both portal APIs.

use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use chrono::{DateTime, SecondsFormat, Utc};

/// A point in time in UTC. On the wire: RFC 3339 with milliseconds and `Z`,
/// e.g. `2026-10-09T01:33:02.172Z`. Input may use any offset; it is converted to UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct UtcTimestamp(pub DateTime<Utc>);

impl From<DateTime<Utc>> for UtcTimestamp {
    fn from(t: DateTime<Utc>) -> Self {
        Self(t)
    }
}

#[Scalar(name = "UtcTimestamp")]
impl ScalarType for UtcTimestamp {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => DateTime::parse_from_rfc3339(s)
                .map(|t| Self(t.with_timezone(&Utc)))
                .map_err(|_| InputValueError::custom("expected an RFC 3339 timestamp, e.g. 2026-10-09T01:33:02.172Z")),
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_rfc3339_opts(SecondsFormat::Millis, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc_with_millis_and_parses_offsets() {
        let t = UtcTimestamp(DateTime::parse_from_rfc3339("2026-10-09T01:33:02.172Z").unwrap().with_timezone(&Utc));
        assert_eq!(t.to_value(), Value::String("2026-10-09T01:33:02.172Z".into()));
        let parsed = UtcTimestamp::parse(Value::String("2026-10-08T21:33:02.172-04:00".into())).unwrap();
        assert_eq!(parsed, t);
        assert!(UtcTimestamp::parse(Value::String("yesterday".into())).is_err());
    }
}
