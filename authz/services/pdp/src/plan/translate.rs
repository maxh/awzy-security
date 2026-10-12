//! From Cedar's residuals to a plan's filter (ADR 0113). A residual is a policy's condition with
//! the resource left unknown, read here in Cedar's JSON policy format, which writes an unknown
//! as a call to `unknown("<name>")`. What refers to the resource becomes a filter node, and what
//! the tree can't express becomes `unmappable`, never a guess.

use awzy_model::EntityRef;
use serde_json::{Map, Value};

use super::filter::{Cmp, Filter, Literal};

/// The unknown the resource is, in residuals.
const RESOURCE: &str = "resource";

/// Expands a principal to itself and its ancestors, for `principal in resource.<groups>`.
pub trait Ancestry {
    fn with_ancestors(&self, entity: &EntityRef) -> Vec<EntityRef>;
}

/// A residual policy's conditions (Cedar's JSON policy format) as one filter over the resource:
/// every `when` holds and no `unless` does.
pub fn policy_filter(policy: &Value, ancestry: &dyn Ancestry) -> Filter {
    let t = Translator { ancestry };
    let Some(conditions) = policy.get("conditions").and_then(Value::as_array) else { return Filter::True };
    Filter::and(conditions.iter().map(|c| {
        let body = c.get("body").unwrap_or(&Value::Null);
        match c.get("kind").and_then(Value::as_str) {
            Some("when") => t.boolean(body),
            Some("unless") => Filter::not(t.boolean(body)),
            _ => Translator::unmappable(c),
        }
    }))
}

/// One condition expression as a filter.
pub fn filter(expr: &Value, ancestry: &dyn Ancestry) -> Filter {
    Translator { ancestry }.boolean(expr)
}

struct Translator<'a> {
    ancestry: &'a dyn Ancestry,
}

enum Operand {
    /// A path into the resource's attributes.
    Attr(Vec<String>),
    /// The resource itself.
    Resource,
    Lit(Literal),
    Set(Vec<Literal>),
    Other,
}

/// An expression's one key and its body: `{"==": {...}}` is `("==", {...})`.
fn node(expr: &Value) -> Option<(&str, &Value)> {
    let obj = expr.as_object()?;
    if obj.len() != 1 {
        return None;
    }
    obj.iter().next().map(|(k, v)| (k.as_str(), v))
}

fn field<'a>(body: &'a Value, key: &str) -> &'a Value {
    body.get(key).unwrap_or(&Value::Null)
}

/// An entity in Cedar's JSON: `{"__entity": {"type", "id"}}`, or `{"type", "id"}`.
fn entity(v: &Map<String, Value>) -> Option<EntityRef> {
    let e = v.get("__entity").and_then(Value::as_object).unwrap_or(v);
    Some(EntityRef::new(e.get("type")?.as_str()?, e.get("id")?.as_str()?))
}

/// A value literal (`{"Value": …}`'s body) as a filter literal or set of them.
fn value(v: &Value) -> Operand {
    match v {
        Value::Bool(b) => Operand::Lit(Literal::Bool(*b)),
        Value::Number(n) => n.as_i64().map_or(Operand::Other, |n| Operand::Lit(Literal::Long(n))),
        Value::String(s) => Operand::Lit(Literal::String(s.clone())),
        Value::Object(o) => entity(o).map_or(Operand::Other, |e| Operand::Lit(Literal::Entity(e))),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match value(item) {
                    Operand::Lit(l) => out.push(l),
                    _ => return Operand::Other,
                }
            }
            Operand::Set(out)
        }
        Value::Null => Operand::Other,
    }
}

impl Translator<'_> {
    fn unmappable(expr: &Value) -> Filter {
        Filter::Unmappable { cedar: expr.to_string() }
    }

    fn boolean(&self, expr: &Value) -> Filter {
        let Some((op, body)) = node(expr) else { return Self::unmappable(expr) };
        match op {
            "Value" => match body.as_bool() {
                Some(true) => Filter::True,
                Some(false) => Filter::False,
                None => Self::unmappable(expr),
            },
            "unknown" => match unknown_name(body) {
                Some(name) if name != RESOURCE => Filter::Unknown { name: context_name(&name) },
                _ => Self::unmappable(expr),
            },
            "!" => Filter::not(self.boolean(field(body, "arg"))),
            "&&" => Filter::and([self.boolean(field(body, "left")), self.boolean(field(body, "right"))]),
            "||" => Filter::or([self.boolean(field(body, "left")), self.boolean(field(body, "right"))]),
            "if-then-else" => {
                let cond = self.boolean(field(body, "if"));
                Filter::or([
                    Filter::and([cond.clone(), self.boolean(field(body, "then"))]),
                    Filter::and([Filter::not(cond), self.boolean(field(body, "else"))]),
                ])
            }
            "==" | "!=" | "<" | "<=" | ">" | ">=" => self.compare(expr, op, body),
            "in" => self.membership_or_contains(expr, body),
            "contains" | "containsAny" | "containsAll" => self.sets(expr, op, body),
            "has" => {
                let attrs: Vec<String> = match field(body, "attr") {
                    Value::String(a) => vec![a.clone()],
                    Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
                    _ => return Self::unmappable(expr),
                };
                match self.operand(field(body, "left")) {
                    Operand::Attr(mut path) => {
                        path.extend(attrs);
                        Filter::Has { attr: path }
                    }
                    Operand::Resource => Filter::Has { attr: attrs },
                    _ => Self::unmappable(expr),
                }
            }
            "like" => match (self.operand(field(body, "left")), field(body, "pattern").as_array()) {
                (Operand::Attr(attr), Some(pattern)) => match pattern_text(pattern) {
                    Some(pattern) => Filter::Like { attr, pattern },
                    None => Self::unmappable(expr),
                },
                _ => Self::unmappable(expr),
            },
            // The resource's type is known, so only a hierarchy check can be left.
            "is" => match (self.operand(field(body, "left")), body.get("in")) {
                (Operand::Resource, Some(parent)) => self.membership(expr, parent),
                _ => Self::unmappable(expr),
            },
            _ => Self::unmappable(expr),
        }
    }

    fn compare(&self, whole: &Value, op: &str, body: &Value) -> Filter {
        let cmp = match op {
            "==" => Cmp::Eq,
            "!=" => Cmp::Ne,
            "<" => Cmp::Lt,
            "<=" => Cmp::Le,
            ">" => Cmp::Gt,
            _ => Cmp::Ge,
        };
        match (self.operand(field(body, "left")), self.operand(field(body, "right"))) {
            (Operand::Attr(attr), Operand::Lit(value)) => Filter::Compare { attr, cmp, value },
            (Operand::Lit(value), Operand::Attr(attr)) => Filter::Compare { attr, cmp: cmp.flipped(), value },
            // `resource == Type::"id"`: the resource is that one entity.
            (Operand::Resource, Operand::Lit(Literal::Entity(e)))
            | (Operand::Lit(Literal::Entity(e)), Operand::Resource)
                if matches!(cmp, Cmp::Eq | Cmp::Ne) =>
            {
                let only = Filter::Is { id: e };
                if cmp == Cmp::Eq {
                    only
                } else {
                    Filter::not(only)
                }
            }
            _ => Self::unmappable(whole),
        }
    }

    fn membership_or_contains(&self, whole: &Value, body: &Value) -> Filter {
        match self.operand(field(body, "left")) {
            Operand::Resource => self.membership(whole, field(body, "right")),
            // `principal in resource.shared_groups`: the principal, or a group it's in, is in the set.
            Operand::Lit(Literal::Entity(e)) => match self.operand(field(body, "right")) {
                Operand::Attr(attr) => Filter::ContainsAny {
                    attr,
                    values: self.ancestry.with_ancestors(&e).into_iter().map(Literal::Entity).collect(),
                },
                _ => Self::unmappable(whole),
            },
            _ => Self::unmappable(whole),
        }
    }

    fn sets(&self, whole: &Value, op: &str, body: &Value) -> Filter {
        match (op, self.operand(field(body, "left")), self.operand(field(body, "right"))) {
            ("contains", Operand::Attr(attr), Operand::Lit(value)) => Filter::Contains { attr, value },
            ("containsAny", Operand::Attr(attr), Operand::Set(values)) => Filter::ContainsAny { attr, values },
            ("containsAll", Operand::Attr(attr), Operand::Set(values)) => Filter::ContainsAll { attr, values },
            _ => Self::unmappable(whole),
        }
    }

    /// `resource in <parent>`, for one entity or a set of them.
    fn membership(&self, whole: &Value, parent: &Value) -> Filter {
        match self.operand(parent) {
            Operand::Lit(Literal::Entity(ancestor)) => Filter::In { ancestor },
            Operand::Set(items) => {
                let mut any = Vec::new();
                for item in items {
                    match item {
                        Literal::Entity(ancestor) => any.push(Filter::In { ancestor }),
                        _ => return Self::unmappable(whole),
                    }
                }
                Filter::or(any)
            }
            _ => Self::unmappable(whole),
        }
    }

    fn operand(&self, expr: &Value) -> Operand {
        let Some((op, body)) = node(expr) else { return Operand::Other };
        match op {
            "unknown" if unknown_name(body).as_deref() == Some(RESOURCE) => Operand::Resource,
            "Var" if body.as_str() == Some(RESOURCE) => Operand::Resource,
            "." => {
                let Some(attr) = body.get("attr").and_then(Value::as_str) else { return Operand::Other };
                match self.operand(field(body, "left")) {
                    Operand::Resource => Operand::Attr(vec![attr.to_string()]),
                    Operand::Attr(mut path) => {
                        path.push(attr.to_string());
                        Operand::Attr(path)
                    }
                    _ => Operand::Other,
                }
            }
            "Value" => value(body),
            "Set" => {
                let Some(items) = body.as_array() else { return Operand::Other };
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match self.operand(item) {
                        Operand::Lit(l) => out.push(l),
                        _ => return Operand::Other,
                    }
                }
                Operand::Set(out)
            }
            _ => Operand::Other,
        }
    }
}

/// `unknown("<name>")`'s name: its one argument, a string value.
fn unknown_name(args: &Value) -> Option<String> {
    let [arg] = args.as_array()?.as_slice() else { return None };
    let (op, body) = node(arg)?;
    (op == "Value").then(|| body.as_str().map(str::to_string)).flatten()
}

/// The pattern as Cedar writes it, `*` for a wildcard and `\*` for a star.
fn pattern_text(pattern: &[Value]) -> Option<String> {
    let mut out = String::new();
    for p in pattern {
        match p {
            Value::String(w) if w == "Wildcard" => out.push('*'),
            _ => {
                let literal = p.get("Literal")?.as_str()?;
                out.push_str(&literal.replace('*', "\\*"));
            }
        }
    }
    Some(out)
}

/// Unknowns the PDP leaves in context are named after the key; say where they come from.
fn context_name(name: &str) -> String {
    if name.contains('.') || name.contains("::") {
        name.to_string()
    } else {
        format!("context.{name}")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use cedar_policy::Policy;
    use serde_json::json;

    use super::*;

    struct Groups;

    impl Ancestry for Groups {
        fn with_ancestors(&self, entity: &EntityRef) -> Vec<EntityRef> {
            vec![entity.clone(), EntityRef::new("Group", "engineering")]
        }
    }

    /// A policy's conditions, as Cedar writes them in JSON; `resource` reads as the unknown.
    fn translated(src: &str) -> Value {
        let policy = Policy::from_str(src).unwrap().to_json().unwrap();
        serde_json::to_value(policy_filter(&policy, &Groups)).unwrap()
    }

    #[test]
    fn comparisons_and_sets_become_filter_nodes() {
        assert_eq!(
            translated(
                r#"permit(principal, action, resource) when { resource.tenant == "acme" && 3 < resource.tier };"#
            ),
            json!({ "op": "and", "args": [
                { "op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme" },
                { "op": "compare", "attr": ["tier"], "cmp": "gt", "value": 3 },
            ] })
        );
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { resource.labels.containsAny(["a", "b"]) };"#),
            json!({ "op": "contains_any", "attr": ["labels"], "values": ["a", "b"] })
        );
        assert_eq!(
            translated(r#"permit(principal, action, resource) unless { resource has archived };"#),
            json!({ "op": "not", "arg": { "op": "has", "attr": ["archived"] } })
        );
    }

    #[test]
    fn hierarchy_membership_becomes_an_in_node() {
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { resource in Resource::"eng-folder" };"#),
            json!({ "op": "in", "ancestor": { "type": "Resource", "id": "eng-folder" } })
        );
    }

    #[test]
    fn a_principal_in_a_set_the_resource_holds_expands_to_its_groups() {
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { Human::"bob" in resource.shared_groups };"#),
            json!({ "op": "contains_any", "attr": ["shared_groups"], "values": [
                { "type": "Human", "id": "bob" }, { "type": "Group", "id": "engineering" },
            ] })
        );
    }

    #[test]
    fn the_unknown_resource_reads_as_the_resource() {
        let residual = json!({ "==": {
            "left": { ".": { "left": { "unknown": [{ "Value": "resource" }] }, "attr": "owner" } },
            "right": { "Value": { "__entity": { "type": "Human", "id": "bob" } } },
        } });
        assert_eq!(
            serde_json::to_value(filter(&residual, &Groups)).unwrap(),
            json!({ "op": "compare", "attr": ["owner"], "cmp": "eq", "value": { "type": "Human", "id": "bob" } })
        );
    }

    #[test]
    fn what_the_tree_cant_express_is_marked_not_guessed() {
        let f = policy_filter(
            &Policy::from_str(r#"permit(principal, action, resource) when { resource.a == resource.b };"#)
                .unwrap()
                .to_json()
                .unwrap(),
            &Groups,
        );
        assert!(matches!(f, Filter::Unmappable { .. }), "{f:?}");
        let assigned = json!({ "unknown": [{ "Value": "assigned" }] });
        assert_eq!(filter(&assigned, &Groups), Filter::Unknown { name: "context.assigned".into() });
    }
}
