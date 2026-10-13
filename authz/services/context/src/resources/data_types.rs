//! What kinds of data a resource holds (ADR 0123), beside its tier: what awzy-classify found, and
//! what a connector or the client's app sent in `attributes.data_types`, which may name the
//! client's own kinds. Policies see both, as `resource.data_types`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{invalid, valid_slug, Resource};
use crate::store::StoreError;

/// One kind of data the classifier found in a resource, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataType {
    /// `customer_personal`, `financial`, … or a kind the client sent.
    pub data_type: String,
    /// label | content | metadata
    pub by: String,
    pub reason: String,
}

impl From<&awzy_classify::DataTypeFinding> for DataType {
    fn from(f: &awzy_classify::DataTypeFinding) -> Self {
        Self { data_type: f.data_type.clone(), by: f.source.as_str().to_string(), reason: f.reason.clone() }
    }
}

/// The part of a resource's `attributes` that names its data types.
#[derive(Debug, Default, Deserialize)]
struct Sent {
    #[serde(default)]
    data_types: Vec<String>,
}

/// The data types a connector or app sent with the resource; none if `attributes` has none.
pub fn sent(attributes: &Value) -> Vec<String> {
    serde_json::from_value::<Sent>(attributes.clone()).unwrap_or_default().data_types
}

/// Sent data types are lowercase slugs, as policies compare them: `customer_personal`.
pub(super) fn validate_sent(id: &str, attributes: &Value) -> Result<(), StoreError> {
    if attributes.is_null() {
        return Ok(());
    }
    let ok = serde_json::from_value::<Sent>(attributes.clone())
        .is_ok_and(|s| s.data_types.iter().all(|t| valid_slug(t) && !t.contains('-')));
    if ok {
        Ok(())
    } else {
        Err(invalid(format!("{id}: attributes.data_types is a list of lowercase names like customer_personal")))
    }
}

impl Resource {
    /// The data types policies see: what was sent and what the classifier found, sorted, each once.
    pub fn effective_data_types(&self) -> Vec<String> {
        let found = self.classification.iter().flat_map(|c| c.data_types.iter().map(|d| d.data_type.clone()));
        let all: std::collections::BTreeSet<String> = sent(&self.attributes).into_iter().chain(found).collect();
        all.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn sent_data_types_are_lowercase_names() {
        assert!(validate_sent("r", &json!({ "data_types": ["customer_personal", "trade_secrets"] })).is_ok());
        assert!(validate_sent("r", &json!({ "team": "sales" })).is_ok(), "other attributes are the app's own");
        assert!(validate_sent("r", &Value::Null).is_ok());
        assert!(validate_sent("r", &json!({ "data_types": ["Customer PII"] })).is_err());
        assert!(validate_sent("r", &json!({ "data_types": "financial" })).is_err(), "a list, not a string");
        assert_eq!(sent(&json!({ "data_types": ["legal"] })), vec!["legal"]);
    }
}
