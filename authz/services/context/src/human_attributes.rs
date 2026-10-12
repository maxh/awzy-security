//! What policies can say about a person beyond their groups: the HR attributes the client's IdP
//! sends over SCIM, in the core schema and its enterprise extension (RFC 7643 sections 4.1 and
//! 4.3), read from the resource awzy-context keeps for each person (ADR 0116).

use serde::Deserialize;
use serde_json::{Map, Value};

/// The SCIM enterprise user extension, which carries department, division, and manager.
pub const ENTERPRISE_SCHEMA: &str = "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User";

/// The fields read from a person's SCIM resource. Anything else the IdP sends is kept on the
/// resource but isn't an attribute.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ScimUser {
    title: Option<Value>,
    #[serde(rename = "userType")]
    user_type: Option<Value>,
    /// Parsed on its own, so a malformed extension doesn't lose the core attributes.
    #[serde(rename = "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User")]
    enterprise: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Enterprise {
    department: Option<Value>,
    division: Option<Value>,
    organization: Option<Value>,
    #[serde(rename = "costCenter")]
    cost_center: Option<Value>,
    /// Not in RFC 7643, but where IdPs that map an employee type into the extension put it.
    #[serde(rename = "employeeType")]
    employee_type: Option<Value>,
}

/// The longest value kept; an IdP field longer than this isn't one a policy compares against.
const MAX_LEN: usize = 256;

fn text(v: Option<Value>) -> Option<String> {
    let s = v?.as_str()?.trim().to_string();
    (!s.is_empty() && s.len() <= MAX_LEN).then_some(s)
}

/// An employee type as policies compare it: "Contractor" and "contractor" alike are
/// `contractor`, and "Full Time" is `full_time`.
fn slug(s: String) -> String {
    s.to_lowercase().split_whitespace().collect::<Vec<_>>().join("_").replace('-', "_")
}

/// The person's HR attributes as Cedar entity attributes, each only when the IdP sent it:
/// `title`, `department`, `division`, `organization`, `cost_center`, and `employee_type` (SCIM
/// `userType`, else the enterprise extension's `employeeType`, as a lowercase slug).
pub fn of(resource: &Value) -> Map<String, Value> {
    let user = ScimUser::deserialize(resource).unwrap_or_default();
    let ext = user.enterprise.and_then(|e| Enterprise::deserialize(e).ok()).unwrap_or_default();
    let employee_type = text(user.user_type).or_else(|| text(ext.employee_type)).map(slug);
    [
        ("title", text(user.title)),
        ("department", text(ext.department)),
        ("division", text(ext.division)),
        ("organization", text(ext.organization)),
        ("cost_center", text(ext.cost_center)),
        ("employee_type", employee_type),
    ]
    .into_iter()
    .filter_map(|(k, v)| Some((k.to_string(), Value::String(v?))))
    .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_okta_user_has_its_title_department_and_employee_type() {
        let okta = json!({
            "userName": "sam@example.com",
            "title": "Software Engineer",
            "userType": "Contractor",
            ENTERPRISE_SCHEMA: {
                "department": "engineering",
                "costCenter": "R&D-1",
                "organization": "Example Co",
                "manager": { "value": "123" },
            },
        });
        assert_eq!(
            Value::Object(of(&okta)),
            json!({
                "title": "Software Engineer",
                "employee_type": "contractor",
                "department": "engineering",
                "cost_center": "R&D-1",
                "organization": "Example Co",
            })
        );
    }

    #[test]
    fn what_the_idp_leaves_out_is_absent_not_empty() {
        assert!(of(&json!({ "userName": "a@example.com" })).is_empty());
        assert!(of(&json!({ "userName": "a@example.com", "title": "  ", ENTERPRISE_SCHEMA: {} })).is_empty());
    }

    #[test]
    fn a_value_of_the_wrong_type_is_ignored_not_an_error() {
        let odd = json!({ "title": 7, "userType": ["Employee"], ENTERPRISE_SCHEMA: { "department": "sales" } });
        assert_eq!(Value::Object(of(&odd)), json!({ "department": "sales" }));
        let bad_extension = json!({ "title": "Analyst", ENTERPRISE_SCHEMA: "not an object" });
        assert_eq!(Value::Object(of(&bad_extension)), json!({ "title": "Analyst" }));
    }

    #[test]
    fn employee_types_are_compared_as_slugs() {
        let full_time = json!({ "userType": " Full-Time Employee " });
        assert_eq!(of(&full_time)["employee_type"], "full_time_employee");
        let ext = json!({ ENTERPRISE_SCHEMA: { "employeeType": "Intern" } });
        assert_eq!(of(&ext)["employee_type"], "intern");
    }
}
