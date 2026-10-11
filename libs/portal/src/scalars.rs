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

/// A JSON object, sent as an object rather than text, so the frontend reads it without parsing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonObject(pub serde_json::Map<String, serde_json::Value>);

impl JsonObject {
    /// `value` if it's an object; anything else isn't one.
    pub fn from_json(value: serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Object(o) => Some(Self(o)),
            _ => None,
        }
    }
}

#[Scalar(name = "JSONObject")]
impl ScalarType for JsonObject {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::Object(_) => match value.into_json() {
                Ok(serde_json::Value::Object(o)) => Ok(Self(o)),
                _ => Err(InputValueError::custom("expected a JSON object")),
            },
            other => Err(InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::from_json(serde_json::Value::Object(self.0.clone())).unwrap_or(Value::Null)
    }
}

/// JSON as text, for where the exact text matters: a download shown as it will be saved, or
/// options handed to a browser API. Always valid JSON, in and out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonString(String);

impl JsonString {
    /// `value`'s JSON text.
    pub fn of(value: &serde_json::Value) -> Self {
        Self(value.to_string())
    }

    /// `text`, if it's valid JSON.
    pub fn parse_text(text: impl Into<String>) -> Result<Self, serde_json::Error> {
        let text = text.into();
        serde_json::from_str::<serde_json::Value>(&text)?;
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[Scalar(name = "JSONString")]
impl ScalarType for JsonString {
    fn parse(value: Value) -> InputValueResult<Self> {
        match &value {
            Value::String(s) => {
                Self::parse_text(s.clone()).map_err(|e| InputValueError::custom(format!("not JSON: {e}")))
            }
            _ => Err(InputValueError::expected_type(value)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_json_object_is_an_object_on_the_wire_and_nothing_else() {
        let o = JsonObject::from_json(serde_json::json!({ "ip": "10.0.0.1", "n": 2 })).unwrap();
        let wire = o.to_value();
        assert!(matches!(wire, Value::Object(_)));
        assert_eq!(JsonObject::parse(wire).unwrap(), o);
        assert!(JsonObject::from_json(serde_json::json!([1, 2])).is_none());
        assert!(JsonObject::parse(Value::String("{}".into())).is_err(), "text isn't an object");
    }

    #[test]
    fn a_json_string_is_valid_json_text() {
        let s = JsonString::of(&serde_json::json!([{ "kind": "group", "id": "eng" }]));
        assert_eq!(s.as_str(), r#"[{"id":"eng","kind":"group"}]"#);
        assert_eq!(JsonString::parse(Value::String("{\"a\":1}".into())).unwrap().as_str(), "{\"a\":1}");
        assert!(JsonString::parse(Value::String("not json".into())).is_err());
    }

    #[test]
    fn formats_utc_with_millis_and_parses_offsets() {
        let t = UtcTimestamp(DateTime::parse_from_rfc3339("2026-10-09T01:33:02.172Z").unwrap().with_timezone(&Utc));
        assert_eq!(t.to_value(), Value::String("2026-10-09T01:33:02.172Z".into()));
        let parsed = UtcTimestamp::parse(Value::String("2026-10-08T21:33:02.172-04:00".into())).unwrap();
        assert_eq!(parsed, t);
        assert!(UtcTimestamp::parse(Value::String("yesterday".into())).is_err());
    }
}
