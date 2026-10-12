use std::collections::HashSet;

use awzy_model::{Check, Verdict};
use serde_json::{json, Value};

use super::*;
use crate::{DEFAULT_POLICIES, DEFAULT_SCHEMA};

/// awzy's schema, plus an app's customers in regions, which agents list (ADR 0108).
fn schema() -> String {
    format!(
        "{DEFAULT_SCHEMA}\n\
         @awzy(\"app\")\nentity Region in [Region];\n\
         @awzy(\"app\")\nentity Customer in [Region] {{ tenant: String, tier: Long, owner: Human, labels: Set<String> }};\n\
         action view_customer appliesTo {{ principal: [Human, ServiceAccount], resource: [Customer], context: RequestContext }};"
    )
}

fn pdp(policies: &str) -> Pdp {
    Pdp::new(&schema(), &format!("{DEFAULT_POLICIES}\n{policies}")).unwrap()
}

/// alice, in `groups`; bob; the agent `bot`, owned by alice; and the regions.
fn people(groups: &[&str]) -> Vec<Value> {
    let parents: Vec<Value> = groups.iter().map(|g| json!({ "type": "Group", "id": g })).collect();
    let mut out = vec![
        json!({ "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": parents }),
        json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [] }),
        json!({ "uid": { "type": "ServiceAccount", "id": "bot" },
                "attrs": { "owner": { "__entity": { "type": "Human", "id": "alice" } }, "ai": true }, "parents": [] }),
        json!({ "uid": { "type": "Region", "id": "emea" }, "attrs": {}, "parents": [] }),
        json!({ "uid": { "type": "Region", "id": "uk" }, "attrs": {}, "parents": [{ "type": "Region", "id": "emea" }] }),
        json!({ "uid": { "type": "Region", "id": "us" }, "attrs": {}, "parents": [] }),
    ];
    out.extend(
        ["support", "sales"].iter().map(|g| json!({ "uid": { "type": "Group", "id": g }, "attrs": {}, "parents": [] })),
    );
    out
}

fn request(delegated: bool, risk: i64) -> PlanRequest {
    PlanRequest {
        principal: if delegated { EntityRef::new("ServiceAccount", "bot") } else { EntityRef::new("Human", "alice") },
        on_behalf_of: delegated.then(|| EntityRef::new("Human", "alice")),
        action: "view_customer".into(),
        resource_type: "Customer".into(),
        context: json!({ "risk_score": risk }).as_object().unwrap().clone(),
        human_present: None,
        plan_id: None,
    }
}

fn planned(pdp: &Pdp, req: &PlanRequest, groups: &[&str]) -> Planned {
    pdp.plan(req, &pdp.entities(Value::Array(people(groups))).unwrap(), &Facts::default()).unwrap()
}

#[test]
fn a_permit_on_the_type_alone_needs_no_filter() {
    let p = pdp("permit (principal, action == Action::\"view_customer\", resource is Customer);");
    assert_eq!(planned(&p, &request(false, 0), &[]).outcome, Outcome::AlwaysAllowed);
}

#[test]
fn nothing_permitted_short_circuits() {
    let p = pdp("permit (principal in Group::\"support\", action == Action::\"view_customer\", resource is Customer);");
    let plan = planned(&p, &request(false, 0), &["sales"]);
    assert_eq!((plan.outcome, plan.filter), (Outcome::AlwaysDenied, Filter::False));
}

#[test]
fn a_condition_on_the_resource_becomes_the_filter() {
    let p = pdp(
        "permit (principal, action == Action::\"view_customer\", resource is Customer) when { resource.tenant == \"acme\" };\n\
         forbid (principal, action == Action::\"view_customer\", resource is Customer) when { resource.labels.contains(\"legal-hold\") };",
    );
    let plan = planned(&p, &request(false, 0), &[]);
    assert_eq!(plan.outcome, Outcome::Conditional);
    assert_eq!(
        serde_json::to_value(&plan.filter).unwrap(),
        json!({ "op": "and", "args": [
            { "op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme" },
            { "op": "not", "arg": { "op": "contains", "attr": ["labels"], "value": "legal-hold" } },
        ] })
    );
}

#[test]
fn an_agent_sees_only_what_both_it_and_its_person_may() {
    let p = pdp(
        "permit (principal, action == Action::\"view_customer\", resource is Customer) when { resource.owner == Human::\"alice\" };\n\
         forbid (principal is ServiceAccount, action == Action::\"view_customer\", resource is Customer) when { resource.tier >= 3 };",
    );
    let alone = planned(&p, &request(false, 0), &[]);
    let delegated = planned(&p, &request(true, 0), &[]);
    assert!(!serde_json::to_string(&alone.filter).unwrap().contains("tier"), "{:?}", alone.filter);
    assert!(serde_json::to_string(&delegated.filter).unwrap().contains("tier"), "{:?}", delegated.filter);
}

#[test]
fn a_challenge_on_context_alone_is_said_up_front() {
    let p = pdp(
        "permit (principal, action == Action::\"view_customer\", resource is Customer);\n\
         @challenge(\"verification\")\n\
         forbid (principal, action == Action::\"view_customer\", resource is Customer) when { context.risk_score > 80 };",
    );
    assert_eq!(planned(&p, &request(false, 0), &[]).outcome, Outcome::AlwaysAllowed);
    let risky = planned(&p, &request(false, 90), &[]);
    assert_eq!(risky.outcome, Outcome::Challenge);
    assert_eq!(risky.challenges.into_iter().collect::<Vec<_>>(), vec!["verification"]);
}

#[test]
fn a_challenge_on_the_resource_hides_rows_and_says_so() {
    let p = pdp("permit (principal, action == Action::\"view_customer\", resource is Customer);\n\
         @challenge(\"approval\")\n\
         forbid (principal, action == Action::\"view_customer\", resource is Customer) when { resource.tier >= 4 };");
    let plan = planned(&p, &request(false, 0), &[]);
    assert_eq!(plan.outcome, Outcome::Conditional);
    assert!(plan.challenge_could_widen);
}

#[test]
fn awzy_resources_leave_assigned_unknown() {
    let p = pdp("");
    let mut req = request(false, 0);
    req.action = "read".into();
    req.resource_type = "Resource".into();
    let plan = planned(&p, &req, &["engineering"]);
    assert_ne!(plan.outcome, Outcome::AlwaysAllowed);
    assert!(plan.filter.attributes().iter().any(|a| a == "sensitivity"), "{:?}", plan.filter);
}

#[test]
fn an_unknown_type_is_refused() {
    let p = pdp("");
    let mut req = request(false, 0);
    req.resource_type = "Invoice".into();
    let err = p.plan(&req, &p.entities(Value::Array(people(&[]))).unwrap(), &Facts::default()).unwrap_err();
    assert!(err.to_string().contains("isn't an entity type"), "{err}");
}

#[test]
fn a_pinned_plan_holds_across_versions_but_not_a_containment() {
    let cache = PlanCache::default();
    let planned = Planned {
        outcome: Outcome::AlwaysAllowed,
        filter: Filter::True,
        challenges: BTreeSet::new(),
        challenge_could_widen: false,
        policies: vec![],
    };
    let v1 = Versions { policy: 1, principals: 1, containments: 1 };
    let id = PlanId::new();
    cache.put("k".into(), id, planned, v1);
    assert!(cache.get("k", None, v1).is_some());
    let risk_moved = Versions { principals: 2, ..v1 };
    assert!(cache.get("k", None, risk_moved).is_none(), "a risk change drops it");
    assert!(cache.get("k", Some(id), risk_moved).is_some_and(|(_, _, _, pinned)| pinned), "the next page keeps it");
    let contained = Versions { containments: 2, ..risk_moved };
    assert!(cache.get("k", Some(id), contained).is_none(), "a containment ends the pin");
}

// ---- The property: a filter never shows a row a check of the same request would deny.

/// A small deterministic generator, so a failure reproduces from its seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

const TENANTS: &[&str] = &["acme", "globex"];
const REGIONS: &[&str] = &["emea", "uk", "us"];
const LABELS: &[&str] = &["vip", "legal-hold", "trial"];
const OWNERS: &[&str] = &["alice", "bob"];

fn random_policy(rng: &mut Rng, n: usize) -> String {
    let head = |effect: &str, principal: &str| {
        format!("@id(\"p{n}\")\n{effect} ({principal}, action == Action::\"view_customer\", resource is Customer)")
    };
    let t = rng.pick(TENANTS);
    let r = rng.pick(REGIONS);
    let l = rng.pick(LABELS);
    let o = rng.pick(OWNERS);
    let tier = rng.below(6);
    match rng.below(12) {
        0 => format!("{} when {{ resource.tenant == \"{t}\" }};", head("permit", "principal")),
        1 => format!("{} when {{ resource.owner == principal }};", head("permit", "principal")),
        2 => format!("{} when {{ resource in Region::\"{r}\" }};", head("permit", "principal")),
        3 => format!("{} when {{ resource.tier < {tier} }};", head("permit", "principal")),
        4 => format!("{};", head("permit", "principal in Group::\"support\"")),
        5 => format!("{} when {{ resource.labels.contains(\"{l}\") }};", head("forbid", "principal")),
        6 => format!("{} when {{ resource.tier >= {tier} }};", head("forbid", "principal is ServiceAccount")),
        7 => {
            format!("@challenge(\"approval\")\n{} when {{ resource.tenant == \"{t}\" }};", head("forbid", "principal"))
        }
        8 => {
            format!("@challenge(\"verification\")\n{} when {{ context.risk_score > 50 }};", head("forbid", "principal"))
        }
        9 => format!(
            "{} when {{ resource.owner == Human::\"{o}\" }};",
            head("forbid", "principal == ServiceAccount::\"bot\"")
        ),
        10 => format!(
            "{} when {{ resource.labels.containsAny([\"{l}\", \"vip\"]) || resource.tenant != \"{t}\" }};",
            head("permit", "principal")
        ),
        _ => format!("{} unless {{ resource in Region::\"{r}\" }};", head("forbid", "principal is ServiceAccount")),
    }
}

fn random_customer(rng: &mut Rng, i: usize) -> Value {
    let labels: Vec<&str> = LABELS.iter().copied().filter(|_| rng.below(3) == 0).collect();
    json!({
        "uid": { "type": "Customer", "id": format!("c{i}") },
        "attrs": {
            "tenant": rng.pick(TENANTS),
            "tier": rng.below(6) as i64,
            "owner": { "__entity": { "type": "Human", "id": rng.pick(OWNERS) } },
            "labels": labels,
        },
        "parents": [{ "type": "Region", "id": rng.pick(REGIONS) }],
    })
}

/// A Customer's ancestors, through the regions' own hierarchy.
fn ancestors(customer: &Value) -> Vec<EntityRef> {
    let region = customer["parents"][0]["id"].as_str().unwrap();
    let mut out = vec![EntityRef::new("Region", region)];
    if region == "uk" {
        out.push(EntityRef::new("Region", "emea"));
    }
    out
}

#[test]
fn a_filter_never_matches_a_row_a_check_would_deny() {
    let mut rng = Rng(0x5eed_cafe_f00d_0001);
    let mut cases = 0;
    for case in 0..150 {
        let policies: Vec<String> = (0..1 + rng.below(5) as usize).map(|n| random_policy(&mut rng, n)).collect();
        let p = pdp(&policies.join("\n"));
        let delegated = rng.below(2) == 0;
        let risk = *rng.pick(&[0, 90]);
        let groups: Vec<&str> = ["support", "sales"].into_iter().filter(|_| rng.below(2) == 0).collect();
        let req = request(delegated, risk);
        let plan = planned(&p, &req, &groups);
        for i in 0..12 {
            let customer = random_customer(&mut rng, i);
            let mut all = people(&groups);
            all.push(customer.clone());
            let entities = p.entities(Value::Array(all)).unwrap();
            let check = Check {
                principal: req.principal.clone(),
                action: req.action.clone(),
                resource: EntityRef::new("Customer", format!("c{i}")),
                context: req.context.clone(),
                on_behalf_of: req.on_behalf_of.clone(),
                human_present: None,
                always_enforced: false,
            };
            let verdict = p.decide_passed(&check, &entities, None, &HashSet::new()).unwrap().verdict;
            let id = EntityRef::new("Customer", format!("c{i}"));
            let matched = plan.filter.matches(&id, &customer["attrs"], &ancestors(&customer));
            let context =
                format!("case {case}, customer {customer}\npolicies:\n{}\nplan: {plan:?}", policies.join("\n"));
            match plan.outcome {
                Outcome::AlwaysDenied => assert_ne!(verdict, Verdict::Allow, "{context}"),
                Outcome::Challenge if matched == Some(true) => assert_ne!(verdict, Verdict::Deny, "{context}"),
                _ if matched == Some(true) => assert_eq!(verdict, Verdict::Allow, "{context}"),
                _ => {}
            }
            // And it isn't merely safe: a filter with nothing left to check per row shows every
            // row a check allows.
            if verdict == Verdict::Allow && !plan.filter.needs_row_checks() && plan.outcome != Outcome::Challenge {
                assert_eq!(matched, Some(true), "a row the check allows is missing\n{context}");
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 150 * 12);
}
