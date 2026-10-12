//! Where a person sits in the organization: their manager, from the SCIM enterprise extension,
//! and their team, among the groups the client says are teams (ADR 0117).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

/// The groups that are teams, as the client names them in `AWZY_CONTEXT_TEAM_GROUPS`: a
/// comma-separated list of group names, each of which may use `*` for any run of characters
/// (`sales-*`), compared without regard to case. Empty: no group is a team, and nobody has one.
#[derive(Debug, Clone, Default)]
pub struct TeamGroups(Vec<String>);

impl TeamGroups {
    pub fn from_env() -> Self {
        Self::parse(&std::env::var("AWZY_CONTEXT_TEAM_GROUPS").unwrap_or_default())
    }

    pub fn parse(list: &str) -> Self {
        Self(list.split(',').map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()).collect())
    }

    pub fn is_team(&self, group: &str) -> bool {
        let group = group.to_lowercase();
        self.0.iter().any(|pattern| matches(pattern, &group))
    }

    /// A person's team: of the groups they're directly in, the team with the fewest direct
    /// members, ties by name. Someone in "sales-emea" and "sales-leads" is on the smaller.
    pub fn team_of<'a>(&self, groups: &[&'a str], size: &HashMap<&str, usize>) -> Option<&'a str> {
        groups.iter().copied().filter(|g| self.is_team(g)).min_by_key(|g| (size.get(g).copied().unwrap_or(0), *g))
    }
}

/// Whether `text` matches `pattern`, where `*` matches any run of characters.
fn matches(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    // No `*`: one part, which has to be all of it.
    let [first, middle @ .., last] = parts.as_slice() else { return pattern == text };
    let (first, last) = (*first, *last);
    if text.len() < first.len() + last.len() || !text.starts_with(first) || !text.ends_with(last) {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WithEnterprise {
    #[serde(rename = "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User")]
    enterprise: Option<Value>,
}

/// How the IdP named a person's manager: the enterprise extension's `manager.value` (Okta and
/// RFC 7643) or the bare string Entra ID patches it with.
pub fn manager_ref(resource: &Value) -> Option<String> {
    let enterprise = WithEnterprise::deserialize(resource).ok()?.enterprise?;
    let manager = enterprise.get("manager")?;
    let id = match manager {
        Value::String(s) => s.as_str(),
        Value::Object(_) => manager.get("value")?.as_str()?,
        _ => return None,
    };
    let id = id.trim();
    (!id.is_empty()).then(|| id.to_string())
}

/// People by every ID an IdP might give for a manager: their SCIM ID, the external ID the IdP
/// keeps for them, or their user name, in that order of preference, without regard to case.
pub struct Directory<'a> {
    ids: HashMap<String, &'a str>,
}

impl<'a> Directory<'a> {
    /// `humans` are each person's SCIM ID, external ID if any, and user name.
    pub fn new(humans: impl Iterator<Item = (String, Option<&'a str>, &'a str)> + Clone) -> Self {
        let mut ids = HashMap::new();
        // Least preferred first, so a more preferred ID overwrites a clash.
        for (_, _, user_name) in humans.clone() {
            ids.insert(user_name.to_lowercase(), user_name);
        }
        for (_, external_id, user_name) in humans.clone() {
            if let Some(external_id) = external_id {
                ids.insert(external_id.to_lowercase(), user_name);
            }
        }
        for (scim_id, _, user_name) in humans {
            ids.insert(scim_id.to_lowercase(), user_name);
        }
        Self { ids }
    }

    /// The user name of the person `id` names.
    pub fn resolve(&self, id: &str) -> Option<&'a str> {
        self.ids.get(&id.to_lowercase()).copied()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::human_attributes::ENTERPRISE_SCHEMA;

    #[test]
    fn team_groups_match_by_name_or_pattern_ignoring_case() {
        let teams = TeamGroups::parse(" sales-* , Security,, *-ops ");
        assert!(teams.is_team("sales-emea"));
        assert!(teams.is_team("SECURITY"));
        assert!(teams.is_team("cloud-ops"));
        assert!(!teams.is_team("sales"), "the department, not one of its teams");
        assert!(!teams.is_team("everyone"));
        assert!(!TeamGroups::default().is_team("security"), "no setting, no teams");
    }

    #[test]
    fn patterns_need_every_part_in_order() {
        assert!(matches("a*b*c", "a-b-c"));
        assert!(matches("a*b*c", "abc"));
        assert!(!matches("a*b*c", "a-c-b"));
        assert!(!matches("ab*ba", "aba"), "the prefix and suffix can't overlap");
        assert!(matches("*", ""));
    }

    #[test]
    fn a_persons_team_is_their_smallest_team_group() {
        let teams = TeamGroups::parse("sales-*");
        let size = HashMap::from([("sales-emea", 14), ("sales-leads", 3), ("everyone", 200)]);
        assert_eq!(teams.team_of(&["everyone", "sales-emea", "sales-leads"], &size), Some("sales-leads"));
        assert_eq!(teams.team_of(&["everyone"], &size), None);
    }

    #[test]
    fn a_manager_is_named_however_the_idp_sends_it() {
        let okta = json!({ ENTERPRISE_SCHEMA: { "manager": { "value": "123", "displayName": "Pat" } } });
        assert_eq!(manager_ref(&okta).as_deref(), Some("123"));
        let entra = json!({ ENTERPRISE_SCHEMA: { "manager": " abc " } });
        assert_eq!(manager_ref(&entra).as_deref(), Some("abc"));
        assert_eq!(manager_ref(&json!({ ENTERPRISE_SCHEMA: { "manager": { "value": "" } } })), None);
        assert_eq!(manager_ref(&json!({ "userName": "a" })), None);
    }

    #[test]
    fn a_manager_is_found_by_scim_id_then_external_id_then_user_name() {
        let humans = [
            ("0190-aaaa".to_string(), Some("E-1"), "pat@example.com"),
            ("0190-bbbb".to_string(), Some("pat@example.com"), "lee@example.com"),
        ];
        let directory = Directory::new(humans.iter().map(|(s, e, u)| (s.clone(), *e, *u)));
        assert_eq!(directory.resolve("0190-AAAA"), Some("pat@example.com"));
        assert_eq!(directory.resolve("e-1"), Some("pat@example.com"));
        assert_eq!(directory.resolve("pat@example.com"), Some("lee@example.com"), "an external ID wins a clash");
        assert_eq!(directory.resolve("nobody"), None);
    }
}
