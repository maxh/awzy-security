use super::*;
use cedar_policy::PolicyId;
use std::str::FromStr;

const SCHEMA: &str = include_str!("../../../../policies/awzy.cedarschema");
const BASE: &str = include_str!("../../../../policies/base.cedar");
const PROPERTIES: &str = include_str!("../../../../policies/base.properties.cedar");

fn prover() -> Prover {
    let cvc5 = std::env::var("CVC5").expect("CVC5 names the cvc5 executable; Bazel sets it");
    Prover::new(Path::new(&cvc5)).unwrap()
}

fn schema() -> Schema {
    Schema::from_cedarschema_str(SCHEMA).unwrap().0
}

/// Each property in base.properties.cedar, by name, with whether it holds after challenges.
fn properties() -> Vec<(String, bool, PolicySet)> {
    PolicySet::from_str(PROPERTIES)
        .unwrap()
        .policies()
        .map(|p| {
            let after_challenges = match p.annotation("holds") {
                Some("after_challenges") => true,
                Some("outright") => false,
                other => panic!("{} says it holds {other:?}, not \"outright\" or \"after_challenges\"", name(p)),
            };
            (name(p), after_challenges, PolicySet::from_policies([p.clone()]).unwrap())
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn base_policies_keep_every_property() {
    let schema = schema();
    let base = PolicySet::from_str(BASE).unwrap();
    let after = after_challenges(&base).unwrap();
    let mut prover = prover();
    let mut broken = Vec::new();
    for (id, after_challenges, property) in properties() {
        let policies = if after_challenges { &after } else { &base };
        if let Some(cex) = prover.allowed_outside(&schema, policies, &property).await.unwrap() {
            broken.push(format!("{id}: base.cedar allows\n{cex}"));
        }
    }
    assert!(broken.is_empty(), "properties base.cedar breaks:\n\n{}", broken.join("\n\n"));
}

// A property everything keeps proves nothing.
#[tokio::test(flavor = "current_thread")]
async fn every_property_rules_something_out() {
    let schema = schema();
    let everything = PolicySet::from_str("permit (principal, action, resource);").unwrap();
    let mut prover = prover();
    for (id, _, property) in properties() {
        let cex = prover.allowed_outside(&schema, &everything, &property).await.unwrap();
        assert!(cex.is_some(), "{id} allows every request, so it promises nothing");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn no_base_forbid_errors() {
    let schema = schema();
    let base = PolicySet::from_str(BASE).unwrap();
    let erring = prover().forbids_that_error(&schema, &base).await.unwrap();
    let report: Vec<String> =
        erring.iter().map(|(id, cex)| format!("{id} errors, and so fails open, on\n{cex}")).collect();
    assert!(report.is_empty(), "{}", report.join("\n\n"));
}

// The checks catch what they're for: a permit that lets an agent switch enforcement, and a
// forbid that errors.
#[tokio::test(flavor = "current_thread")]
async fn a_policy_that_breaks_a_property_is_caught() {
    let schema = schema();
    let mut policies = PolicySet::from_str(BASE).unwrap();
    let rogue = Policy::parse(
        Some(PolicyId::new("agents-switch")),
        r#"permit (principal is ServiceAccount, action == Action::"switch_enforcement", resource);"#,
    )
    .unwrap();
    policies.add(rogue).unwrap();
    let (_, _, property) = properties()
        .into_iter()
        .find(|(name, _, _)| name == "agents-never-switch-enforcement-or-configure-connectors")
        .unwrap();
    let after = after_challenges(&policies).unwrap();
    let cex = prover().allowed_outside(&schema, &after, &property).await.unwrap();
    assert!(cex.is_some(), "an agent allowed to switch enforcement wasn't caught");
}

#[tokio::test(flavor = "current_thread")]
async fn a_forbid_that_can_error_is_caught() {
    let schema = schema();
    // The validator rules out reading a missing attribute; what's left is mostly overflow, here
    // on a risk score no one meant to scale this far.
    let policies = PolicySet::from_str(
        r#"@id("scaled-risk")
        forbid (principal, action, resource)
        when { context.risk_score * 1000000000000000000 > 80 };"#,
    )
    .unwrap();
    let erring = prover().forbids_that_error(&schema, &policies).await.unwrap();
    assert_eq!(erring.len(), 1, "a forbid that can overflow wasn't caught");
}
