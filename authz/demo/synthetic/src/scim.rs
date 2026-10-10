//! The SCIM resources an IdP like Okta or Entra ID would send for the company's people and
//! groups, including the enterprise extension (department, manager, employee number).

use serde_json::{json, Value};

use crate::org::{Human, Kind};

pub const USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub const GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
pub const ENTERPRISE_SCHEMA: &str = "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User";

/// `manager_id` is the manager's SCIM ID, known once the manager has been provisioned.
pub fn user(p: &Human, manager_id: Option<&str>) -> Value {
    let mut enterprise = json!({
        "employeeNumber": p.employee_number,
        "organization": "Larkspur Capital",
        "department": p.department,
    });
    if let (Some(id), Some(name)) = (manager_id, &p.manager) {
        enterprise["manager"] = json!({ "value": id, "displayName": name });
    }
    let mut user = json!({
        "schemas": [USER_SCHEMA, ENTERPRISE_SCHEMA],
        "userName": p.user_name,
        "externalId": p.employee_number,
        "active": true,
        "name": { "givenName": p.given_name, "familyName": p.family_name, "formatted": p.display_name() },
        "displayName": p.display_name(),
        "title": p.title,
        "userType": match p.kind { Kind::Employee => "Employee", Kind::Contractor => "Contractor" },
        "emails": [{ "value": p.user_name, "type": "work", "primary": true }],
    });
    user[ENTERPRISE_SCHEMA] = enterprise;
    if let Some(end) = p.contract_end {
        user["urn:larkspur:contract"] = json!({ "endDate": end });
    }
    user
}

/// `member_ids` are the SCIM IDs of the group's people and groups.
pub fn group(name: &str, member_ids: &[String]) -> Value {
    let members: Vec<Value> = member_ids.iter().map(|id| json!({ "value": id })).collect();
    json!({ "schemas": [GROUP_SCHEMA], "displayName": name, "members": members })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::org::{generate, Profile};

    #[test]
    fn users_look_like_an_idp_sent_them() {
        let c = generate(Profile::Demo, 1);
        let contractor = c.humans.iter().find(|p| p.kind == Kind::Contractor && p.manager.is_some()).unwrap();
        let u = user(contractor, Some("mgr-id"));
        assert_eq!(u["userName"], contractor.user_name);
        assert_eq!(u["userType"], "Contractor");
        assert_eq!(u[ENTERPRISE_SCHEMA]["manager"]["value"], "mgr-id");
        assert_eq!(u[ENTERPRISE_SCHEMA]["department"], "engineering");
        assert_eq!(u["urn:larkspur:contract"]["endDate"], "2027-03-31");
        let ceo = c.humans.iter().find(|p| p.manager.is_none()).unwrap();
        assert!(user(ceo, None)[ENTERPRISE_SCHEMA].get("manager").is_none());
    }

    #[test]
    fn groups_list_members_by_id() {
        let g = group("finance", &["a".to_string(), "b".to_string()]);
        assert_eq!(g["displayName"], "finance");
        assert_eq!(g["members"], json!([{ "value": "a" }, { "value": "b" }]));
    }
}
