//! App-owned entity types (ADR 0108): types a client's bundle adds for their app's own data,
//! marked `@awzy("app")` in the schema, whose attributes come from the enforcement point.

use std::collections::BTreeSet;

use cedar_policy::SchemaFragment;
use serde_json::Value;

/// The entity types awzy itself defines, and gets the attributes of from awzy-context or from
/// its own enforcement points. A bundle can't mark these app-owned.
pub const AWZY_TYPES: &[&str] =
    &["Group", "Human", "ServiceAccount", "Resource", "McpServer", "McpTool", "PortalArea", "PortalField"];

/// The annotation that marks a type app-owned, and its value.
const ANNOTATION: (&str, &str) = ("awzy", "app");

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AppTypesError {
    #[error("the schema doesn't parse: {0}")]
    Schema(String),
    #[error(
        "entity type {0} isn't one of awzy's, so mark it @awzy(\"app\"): its attributes then come from the \
         enforcement point that asks"
    )]
    Unmarked(String),
    #[error("entity type {0} is awzy's own, so it can't be marked @awzy(\"app\")")]
    AwzyType(String),
}

/// The app-owned types `schema` declares, by their full name (`Customer`, or `Billing::Invoice`
/// in a namespace). Fails when a type that isn't awzy's is unmarked, or one of awzy's is marked.
pub fn app_types(schema: &str) -> Result<BTreeSet<String>, AppTypesError> {
    let (fragment, _warnings) =
        SchemaFragment::from_cedarschema_str(schema).map_err(|e| AppTypesError::Schema(e.to_string()))?;
    let json = fragment.to_json_value().map_err(|e| AppTypesError::Schema(e.to_string()))?;
    let mut app = BTreeSet::new();
    let Value::Object(namespaces) = json else { return Ok(app) };
    for (namespace, body) in namespaces {
        let Some(Value::Object(types)) = body.get("entityTypes") else { continue };
        for (name, definition) in types {
            let full = if namespace.is_empty() { name.clone() } else { format!("{namespace}::{name}") };
            let marked = definition.get("annotations").and_then(|a| a.get(ANNOTATION.0)).and_then(Value::as_str)
                == Some(ANNOTATION.1);
            let awzys = namespace.is_empty() && AWZY_TYPES.contains(&name.as_str());
            match (awzys, marked) {
                (true, true) => return Err(AppTypesError::AwzyType(full)),
                (false, false) => return Err(AppTypesError::Unmarked(full)),
                (false, true) => {
                    app.insert(full);
                }
                (true, false) => {}
            }
        }
    }
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_base_schema_has_no_app_types_and_passes() {
        assert_eq!(app_types(crate::DEFAULT_SCHEMA), Ok(BTreeSet::new()));
    }

    #[test]
    fn a_marked_type_is_app_owned_in_or_out_of_a_namespace() {
        let schema = format!(
            "{}\n@awzy(\"app\")\nentity Customer {{ tenant: String }};\nnamespace Billing {{ @awzy(\"app\") entity Invoice; }}",
            crate::DEFAULT_SCHEMA
        );
        let found: Vec<String> = app_types(&schema).unwrap().into_iter().collect();
        assert_eq!(found, ["Billing::Invoice", "Customer"]);
    }

    #[test]
    fn an_unmarked_type_or_a_marked_awzy_type_is_refused() {
        let unmarked = format!("{}\nentity Customer;", crate::DEFAULT_SCHEMA);
        assert_eq!(app_types(&unmarked), Err(AppTypesError::Unmarked("Customer".into())));
        let marked = crate::DEFAULT_SCHEMA.replacen("entity McpServer;", "@awzy(\"app\")\nentity McpServer;", 1);
        assert_eq!(app_types(&marked), Err(AppTypesError::AwzyType("McpServer".into())));
    }
}
