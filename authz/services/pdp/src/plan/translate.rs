//! From Cedar's residuals to a plan's filter (ADR 0113). A residual is a policy's condition with
//! the resource left unknown; what refers to it becomes a filter node, and what the tree can't
//! express becomes `unmappable`, never a guess.

use std::sync::Arc;

use awzy_model::EntityRef;
use cedar_policy::pst::{self, BinaryOp, Expr, UnaryOp};

use super::filter::{Cmp, Filter, Literal};

/// The unknown the resource is, in residuals.
const RESOURCE: &str = "resource";

/// Expands a principal to itself and its ancestors, for `principal in resource.<groups>`.
pub trait Ancestry {
    fn with_ancestors(&self, entity: &EntityRef) -> Vec<EntityRef>;
}

pub fn entity(uid: &pst::EntityUID) -> EntityRef {
    EntityRef::new(uid.ty.to_string(), uid.eid.to_string())
}

/// The condition `expr` as a filter over the resource.
pub fn filter(expr: &Expr, ancestry: &dyn Ancestry) -> Filter {
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

impl Translator<'_> {
    fn unmappable(expr: &Expr) -> Filter {
        Filter::Unmappable { cedar: expr.to_string() }
    }

    fn boolean(&self, expr: &Expr) -> Filter {
        match expr {
            Expr::Literal(pst::Literal::Bool(b)) => {
                if *b {
                    Filter::True
                } else {
                    Filter::False
                }
            }
            Expr::Unknown { name } if name.as_str() != RESOURCE => Filter::Unknown { name: unknown_name(name) },
            Expr::UnaryOp { op: UnaryOp::Not, expr: inner } => Filter::not(self.boolean(inner)),
            Expr::BinaryOp { op: BinaryOp::And, left, right } => Filter::and([self.boolean(left), self.boolean(right)]),
            Expr::BinaryOp { op: BinaryOp::Or, left, right } => Filter::or([self.boolean(left), self.boolean(right)]),
            Expr::IfThenElse { cond, then_expr, else_expr } => {
                let cond = self.boolean(cond);
                Filter::or([
                    Filter::and([cond.clone(), self.boolean(then_expr)]),
                    Filter::and([Filter::not(cond), self.boolean(else_expr)]),
                ])
            }
            Expr::BinaryOp { op, left, right } => self.binary(expr, *op, left, right),
            Expr::HasAttr { expr: target, attrs } => match self.operand(target) {
                Operand::Attr(mut path) => {
                    path.extend(attrs.iter().map(ToString::to_string));
                    Filter::Has { attr: path }
                }
                Operand::Resource => Filter::Has { attr: attrs.iter().map(ToString::to_string).collect() },
                _ => Self::unmappable(expr),
            },
            Expr::Like { expr: target, pattern } => match self.operand(target) {
                Operand::Attr(attr) => Filter::Like { attr, pattern: pattern_text(pattern) },
                _ => Self::unmappable(expr),
            },
            // The resource's type is known, so only a hierarchy check can be left.
            Expr::Is { expr: target, in_expr, .. } => match (self.operand(target), in_expr) {
                (Operand::Resource, Some(parent)) => self.membership(expr, parent),
                _ => Self::unmappable(expr),
            },
            _ => Self::unmappable(expr),
        }
    }

    fn binary(&self, whole: &Expr, op: BinaryOp, left: &Arc<Expr>, right: &Arc<Expr>) -> Filter {
        let cmp = match op {
            BinaryOp::Eq => Some(Cmp::Eq),
            BinaryOp::NotEq => Some(Cmp::Ne),
            BinaryOp::Less => Some(Cmp::Lt),
            BinaryOp::LessEq => Some(Cmp::Le),
            BinaryOp::Greater => Some(Cmp::Gt),
            BinaryOp::GreaterEq => Some(Cmp::Ge),
            _ => None,
        };
        if let Some(cmp) = cmp {
            return match (self.operand(left), self.operand(right)) {
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
            };
        }
        match op {
            BinaryOp::In => match self.operand(left) {
                Operand::Resource => self.membership(whole, right),
                // `principal in resource.shared_groups`: the principal, or a group it's in, is in
                // the set.
                Operand::Lit(Literal::Entity(e)) => match self.operand(right) {
                    Operand::Attr(attr) => Filter::ContainsAny {
                        attr,
                        values: self.ancestry.with_ancestors(&e).into_iter().map(Literal::Entity).collect(),
                    },
                    _ => Self::unmappable(whole),
                },
                _ => Self::unmappable(whole),
            },
            BinaryOp::Contains => match (self.operand(left), self.operand(right)) {
                (Operand::Attr(attr), Operand::Lit(value)) => Filter::Contains { attr, value },
                _ => Self::unmappable(whole),
            },
            BinaryOp::ContainsAny => match (self.operand(left), self.operand(right)) {
                (Operand::Attr(attr), Operand::Set(values)) => Filter::ContainsAny { attr, values },
                _ => Self::unmappable(whole),
            },
            BinaryOp::ContainsAll => match (self.operand(left), self.operand(right)) {
                (Operand::Attr(attr), Operand::Set(values)) => Filter::ContainsAll { attr, values },
                _ => Self::unmappable(whole),
            },
            _ => Self::unmappable(whole),
        }
    }

    /// `resource in <parent>`, for one entity or a set of them.
    fn membership(&self, whole: &Expr, parent: &Expr) -> Filter {
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

    fn operand(&self, expr: &Expr) -> Operand {
        match expr {
            Expr::Unknown { name } if name.as_str() == RESOURCE => Operand::Resource,
            Expr::Var(pst::Var::Resource) => Operand::Resource,
            Expr::GetAttr { expr: target, attr } => match self.operand(target) {
                Operand::Resource => Operand::Attr(vec![attr.to_string()]),
                Operand::Attr(mut path) => {
                    path.push(attr.to_string());
                    Operand::Attr(path)
                }
                _ => Operand::Other,
            },
            Expr::Literal(lit) => match literal(lit) {
                Some(l) => Operand::Lit(l),
                None => Operand::Other,
            },
            Expr::Set(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_ref() {
                        Expr::Literal(lit) => match literal(lit) {
                            Some(l) => out.push(l),
                            None => return Operand::Other,
                        },
                        _ => return Operand::Other,
                    }
                }
                Operand::Set(out)
            }
            _ => Operand::Other,
        }
    }
}

fn literal(lit: &pst::Literal) -> Option<Literal> {
    Some(match lit {
        pst::Literal::Bool(b) => Literal::Bool(*b),
        pst::Literal::Long(n) => Literal::Long(*n),
        pst::Literal::String(s) => Literal::String(s.to_string()),
        pst::Literal::EntityUID(uid) => Literal::Entity(entity(uid)),
        #[allow(unreachable_patterns)]
        _ => return None,
    })
}

/// The pattern as Cedar writes it, `*` for a wildcard and `\*` for a star.
fn pattern_text(pattern: &[pst::PatternElem]) -> String {
    let mut out = String::new();
    for p in pattern {
        match p {
            pst::PatternElem::Wildcard => out.push('*'),
            pst::PatternElem::Char('*') => out.push_str("\\*"),
            pst::PatternElem::Char(c) => out.push(*c),
        }
    }
    out
}

/// Unknowns the PDP leaves in context are named after the key; say where they come from.
fn unknown_name(name: &str) -> String {
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

    use super::*;

    struct Groups;

    impl Ancestry for Groups {
        fn with_ancestors(&self, entity: &EntityRef) -> Vec<EntityRef> {
            vec![entity.clone(), EntityRef::new("Group", "engineering")]
        }
    }

    /// The `when` condition of a policy, as a residual would carry it, with `resource` unknown.
    fn condition(src: &str) -> Expr {
        let policy = Policy::from_str(src).unwrap();
        let pst::Policy::Static(p) = policy.to_pst().unwrap() else { panic!("not static") };
        let Some(pst::Clause::When(expr)) = p.body().clauses().first().cloned() else { panic!("no when") };
        let resource = Arc::new(Expr::Unknown { name: RESOURCE.into() });
        replace_resource(&expr, &resource)
    }

    fn replace_resource(expr: &Expr, with: &Arc<Expr>) -> Expr {
        let r = |e: &Arc<Expr>| Arc::new(replace_resource(e, with));
        match expr {
            Expr::Var(pst::Var::Resource) => with.as_ref().clone(),
            Expr::GetAttr { expr, attr } => Expr::GetAttr { expr: r(expr), attr: attr.clone() },
            Expr::HasAttr { expr, attrs } => Expr::HasAttr { expr: r(expr), attrs: attrs.clone() },
            Expr::BinaryOp { op, left, right } => Expr::BinaryOp { op: *op, left: r(left), right: r(right) },
            Expr::UnaryOp { op, expr } => Expr::UnaryOp { op: *op, expr: r(expr) },
            Expr::Like { expr, pattern } => Expr::Like { expr: r(expr), pattern: pattern.clone() },
            other => other.clone(),
        }
    }

    fn translated(src: &str) -> serde_json::Value {
        serde_json::to_value(filter(&condition(src), &Groups)).unwrap()
    }

    #[test]
    fn comparisons_and_sets_become_filter_nodes() {
        assert_eq!(
            translated(
                r#"permit(principal, action, resource) when { resource.tenant == "acme" && 3 < resource.tier };"#
            ),
            serde_json::json!({ "op": "and", "args": [
                { "op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme" },
                { "op": "compare", "attr": ["tier"], "cmp": "gt", "value": 3 },
            ] })
        );
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { resource.labels.containsAny(["a", "b"]) };"#),
            serde_json::json!({ "op": "contains_any", "attr": ["labels"], "values": ["a", "b"] })
        );
    }

    #[test]
    fn hierarchy_membership_becomes_an_in_node() {
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { resource in Resource::"eng-folder" };"#),
            serde_json::json!({ "op": "in", "ancestor": { "type": "Resource", "id": "eng-folder" } })
        );
    }

    #[test]
    fn a_principal_in_a_set_the_resource_holds_expands_to_its_groups() {
        assert_eq!(
            translated(r#"permit(principal, action, resource) when { Human::"bob" in resource.shared_groups };"#),
            serde_json::json!({ "op": "contains_any", "attr": ["shared_groups"], "values": [
                { "type": "Human", "id": "bob" }, { "type": "Group", "id": "engineering" },
            ] })
        );
    }

    #[test]
    fn what_the_tree_cant_express_is_marked_not_guessed() {
        let f =
            filter(&condition(r#"permit(principal, action, resource) when { resource.a == resource.b };"#), &Groups);
        assert!(matches!(f, Filter::Unmappable { .. }), "{f:?}");
        let assigned = Expr::Unknown { name: "assigned".into() };
        assert_eq!(filter(&assigned, &Groups), Filter::Unknown { name: "context.assigned".into() });
    }
}
