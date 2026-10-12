//! The filter a query plan returns (ADR 0113): a small tree an app's mapper turns into its own
//! query language, over the attributes of the resource type it asked about.

use awzy_model::EntityRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A literal a filter compares with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Literal {
    Bool(bool),
    Long(i64),
    String(String),
    Entity(EntityRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Cmp {
    /// The same comparison with its sides swapped: `1 < x` is `x > 1`.
    pub fn flipped(self) -> Self {
        match self {
            Self::Lt => Self::Gt,
            Self::Le => Self::Ge,
            Self::Gt => Self::Lt,
            Self::Ge => Self::Le,
            same => same,
        }
    }
}

/// Which rows of the type a principal may act on. `attr` is a path into the resource's
/// attributes: `["owner"]`, `["address", "country"]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Filter {
    True,
    False,
    And {
        args: Vec<Filter>,
    },
    Or {
        args: Vec<Filter>,
    },
    Not {
        arg: Box<Filter>,
    },
    /// `resource.<attr> <cmp> <value>`.
    Compare {
        attr: Vec<String>,
        cmp: Cmp,
        value: Literal,
    },
    /// `resource has <attr>`.
    Has {
        attr: Vec<String>,
    },
    /// `resource.<attr>.contains(<value>)`, for a set attribute.
    Contains {
        attr: Vec<String>,
        value: Literal,
    },
    /// `resource.<attr>.containsAny([...])`.
    ContainsAny {
        attr: Vec<String>,
        values: Vec<Literal>,
    },
    /// `resource.<attr>.containsAll([...])`.
    ContainsAll {
        attr: Vec<String>,
        values: Vec<Literal>,
    },
    /// `resource.<attr> like "<pattern>"`, where `*` matches any run of characters.
    Like {
        attr: Vec<String>,
        pattern: String,
    },
    /// `resource in <ancestor>`: the resource is the ancestor or under it.
    In {
        ancestor: EntityRef,
    },
    /// `resource == <id>`: the resource is that one entity.
    Is {
        id: EntityRef,
    },
    /// Depends on something awzy works out per resource, such as `context.assigned`: check these
    /// rows one by one.
    Unknown {
        name: String,
    },
    /// A condition the tree can't express, in Cedar: check these rows one by one.
    Unmappable {
        cedar: String,
    },
}

impl Filter {
    pub fn and(args: impl IntoIterator<Item = Filter>) -> Filter {
        let mut out = Vec::new();
        for a in args {
            match a {
                Filter::True => {}
                Filter::False => return Filter::False,
                Filter::And { args } => out.extend(args),
                other => out.push(other),
            }
        }
        dedup(&mut out);
        match out.len() {
            0 => Filter::True,
            1 => out.pop().unwrap_or(Filter::True),
            _ => Filter::And { args: out },
        }
    }

    pub fn or(args: impl IntoIterator<Item = Filter>) -> Filter {
        let mut out = Vec::new();
        for a in args {
            match a {
                Filter::False => {}
                Filter::True => return Filter::True,
                Filter::Or { args } => out.extend(args),
                other => out.push(other),
            }
        }
        dedup(&mut out);
        match out.len() {
            0 => Filter::False,
            1 => out.pop().unwrap_or(Filter::False),
            _ => Filter::Or { args: out },
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn not(arg: Filter) -> Filter {
        match arg {
            Filter::True => Filter::False,
            Filter::False => Filter::True,
            Filter::Not { arg } => *arg,
            other => Filter::Not { arg: Box::new(other) },
        }
    }

    /// Whether some row has to be checked one by one.
    pub fn needs_row_checks(&self) -> bool {
        match self {
            Filter::Unknown { .. } | Filter::Unmappable { .. } => true,
            Filter::And { args } | Filter::Or { args } => args.iter().any(Filter::needs_row_checks),
            Filter::Not { arg } => arg.needs_row_checks(),
            _ => false,
        }
    }

    /// The attribute paths it reads, sorted, for the plan to name (ADR 0113).
    pub fn attributes(&self) -> Vec<String> {
        let mut out = Vec::new();
        self.collect_attributes(&mut out);
        out.sort();
        out.dedup();
        out
    }

    fn collect_attributes(&self, out: &mut Vec<String>) {
        match self {
            Filter::And { args } | Filter::Or { args } => args.iter().for_each(|a| a.collect_attributes(out)),
            Filter::Not { arg } => arg.collect_attributes(out),
            Filter::Compare { attr, .. }
            | Filter::Has { attr }
            | Filter::Contains { attr, .. }
            | Filter::ContainsAny { attr, .. }
            | Filter::ContainsAll { attr, .. }
            | Filter::Like { attr, .. } => out.push(attr.join(".")),
            _ => {}
        }
    }

    /// Whether a resource with these Cedar attributes and ancestors matches: `Some(true)` only
    /// when it definitely does. A missing attribute, a type mismatch, or an unknown node is
    /// `None`, and a row matches only on `Some(true)`, so a filter never matches more than Cedar
    /// would allow. For tests and for apps that filter in memory.
    pub fn matches(&self, id: &EntityRef, attrs: &Value, ancestors: &[EntityRef]) -> Option<bool> {
        match self {
            Filter::True => Some(true),
            Filter::False => Some(false),
            // Like Cedar's `&&` and `||`: a definite answer from the left settles it.
            Filter::And { args } => {
                for a in args {
                    match a.matches(id, attrs, ancestors) {
                        Some(true) => {}
                        other => return other,
                    }
                }
                Some(true)
            }
            Filter::Or { args } => {
                for a in args {
                    match a.matches(id, attrs, ancestors) {
                        Some(false) => {}
                        other => return other,
                    }
                }
                Some(false)
            }
            Filter::Not { arg } => arg.matches(id, attrs, ancestors).map(|b| !b),
            Filter::Compare { attr, cmp, value } => compare(at(attrs, attr)?, *cmp, value),
            Filter::Has { attr } => Some(at(attrs, attr).is_some()),
            Filter::Contains { attr, value } => Some(set(at(attrs, attr)?)?.iter().any(|v| literal_eq(v, value))),
            Filter::ContainsAny { attr, values } => {
                let s = set(at(attrs, attr)?)?;
                Some(values.iter().any(|value| s.iter().any(|v| literal_eq(v, value))))
            }
            Filter::ContainsAll { attr, values } => {
                let s = set(at(attrs, attr)?)?;
                Some(values.iter().all(|value| s.iter().any(|v| literal_eq(v, value))))
            }
            Filter::Like { attr, pattern } => Some(like(at(attrs, attr)?.as_str()?, pattern)),
            Filter::In { ancestor } => Some(ancestor == id || ancestors.contains(ancestor)),
            Filter::Is { id: only } => Some(only == id),
            Filter::Unknown { .. } | Filter::Unmappable { .. } => None,
        }
    }
}

fn dedup(args: &mut Vec<Filter>) {
    let mut seen: Vec<Filter> = Vec::with_capacity(args.len());
    args.retain(|a| {
        if seen.contains(a) {
            false
        } else {
            seen.push(a.clone());
            true
        }
    });
}

fn at<'a>(attrs: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(attrs, |v, key| v.get(key))
}

fn set(v: &Value) -> Option<&Vec<Value>> {
    v.as_array()
}

/// An entity in Cedar's JSON, `{"__entity": {"type", "id"}}` or `{"type", "id"}`.
fn entity(v: &Value) -> Option<EntityRef> {
    let e = v.get("__entity").unwrap_or(v);
    Some(EntityRef::new(e.get("type")?.as_str()?, e.get("id")?.as_str()?))
}

fn literal_eq(v: &Value, lit: &Literal) -> bool {
    match lit {
        Literal::Bool(b) => v.as_bool() == Some(*b),
        Literal::Long(n) => v.as_i64() == Some(*n),
        Literal::String(s) => v.as_str() == Some(s),
        Literal::Entity(e) => entity(v).as_ref() == Some(e),
    }
}

fn compare(v: &Value, cmp: Cmp, lit: &Literal) -> Option<bool> {
    match (cmp, lit) {
        (Cmp::Eq, _) => Some(literal_eq(v, lit)),
        (Cmp::Ne, _) => Some(!literal_eq(v, lit)),
        (_, Literal::Long(n)) => {
            let x = v.as_i64()?;
            Some(match cmp {
                Cmp::Lt => x < *n,
                Cmp::Le => x <= *n,
                Cmp::Gt => x > *n,
                Cmp::Ge => x >= *n,
                Cmp::Eq | Cmp::Ne => unreachable!("handled above"),
            })
        }
        _ => None,
    }
}

/// Cedar's `like`: `*` matches any run of characters; `\*` is a literal star.
fn like(s: &str, pattern: &str) -> bool {
    let mut parts: Vec<String> = vec![String::new()];
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        let literal = match c {
            '\\' if chars.peek() == Some(&'*') => chars.next().unwrap_or('*'),
            '*' => {
                parts.push(String::new());
                continue;
            }
            c => c,
        };
        if let Some(p) = parts.last_mut() {
            p.push(literal);
        }
    }
    let (first, rest) = parts.split_first().map_or(("", &[][..]), |(f, r)| (f.as_str(), r));
    let Some(mut s) = s.strip_prefix(first) else { return false };
    let Some((last, middle)) = rest.split_last() else { return s.is_empty() };
    for part in middle {
        match s.find(part.as_str()) {
            Some(i) => s = &s[i + part.len()..],
            None => return false,
        }
    }
    s.ends_with(last.as_str())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn attr(name: &str) -> Vec<String> {
        vec![name.to_string()]
    }

    #[test]
    fn true_and_false_fold_away() {
        let x = Filter::Has { attr: attr("owner") };
        assert_eq!(Filter::and([Filter::True, x.clone()]), x);
        assert_eq!(Filter::and([Filter::False, x.clone()]), Filter::False);
        assert_eq!(Filter::or([Filter::False, x.clone(), x.clone()]), x);
        assert_eq!(Filter::not(Filter::not(x.clone())), x);
        assert_eq!(Filter::or(Vec::new()), Filter::False);
    }

    #[test]
    fn a_missing_attribute_never_matches_either_way() {
        let id = EntityRef::new("Customer", "c1");
        let f = Filter::Compare { attr: attr("tenant"), cmp: Cmp::Eq, value: Literal::String("acme".into()) };
        assert_eq!(f.matches(&id, &json!({ "tenant": "acme" }), &[]), Some(true));
        assert_eq!(f.matches(&id, &json!({}), &[]), None);
        assert_eq!(Filter::not(f).matches(&id, &json!({}), &[]), None, "not unknown is unknown");
    }

    #[test]
    fn hierarchy_membership_includes_the_resource_itself() {
        let folder = EntityRef::new("Resource", "folder");
        let f = Filter::In { ancestor: folder.clone() };
        assert_eq!(f.matches(&folder, &json!({}), &[]), Some(true));
        assert_eq!(f.matches(&EntityRef::new("Resource", "doc"), &json!({}), &[folder.clone()]), Some(true));
        assert_eq!(f.matches(&EntityRef::new("Resource", "other"), &json!({}), &[]), Some(false));
    }

    #[test]
    fn like_matches_as_cedar_does() {
        assert!(like("report.pdf", "*.pdf"));
        assert!(like("a-b-c", "a*c"));
        assert!(!like("a-b-d", "a*c"));
        assert!(like("star*", "star\\*"));
        assert!(!like("stars", "star\\*"));
        assert!(like("exact", "exact"));
    }

    #[test]
    fn it_serializes_as_documented() {
        let f = Filter::and([
            Filter::Compare { attr: attr("tenant"), cmp: Cmp::Eq, value: Literal::String("acme".into()) },
            Filter::In { ancestor: EntityRef::new("Folder", "eng") },
        ]);
        assert_eq!(
            serde_json::to_value(&f).unwrap(),
            json!({ "op": "and", "args": [
                { "op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme" },
                { "op": "in", "ancestor": { "type": "Folder", "id": "eng" } },
            ] })
        );
    }
}
