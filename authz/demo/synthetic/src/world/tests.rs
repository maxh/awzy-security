use std::collections::{HashMap, HashSet};

use super::*;
use crate::org::generate;

fn world(seed: u64) -> (Company, Vec<Resource>, Vec<WorkItem>) {
    let c = generate(Profile::Demo, seed);
    let mut all = folders_and_repositories(&c);
    all.extend(scenario_resources(&c));
    for i in 0..c.humans.len() {
        all.extend(human_documents(&c, Profile::Demo, seed, i));
    }
    let work = work_items(&c, Profile::Demo, seed);
    (c, all, work)
}

#[test]
fn the_same_seed_gives_the_same_world() {
    let (_, a, wa) = world(1);
    let (_, b, wb) = world(1);
    assert_eq!(a, b);
    assert_eq!(wa, wb);
    assert_ne!(a, world(2).1);
}

#[test]
fn the_demo_world_has_the_planned_size_and_mix() {
    let (_, all, work) = world(1);
    let docs: Vec<&Resource> = all.iter().filter(|r| r.kind == "document").collect();
    assert!((3_500..=5_000).contains(&docs.len()), "{}", docs.len());
    let share = |t: Tier| docs.iter().filter(|d| d.truth == t).count() * 100 / docs.len();
    assert!((2..=15).contains(&share(Tier::Public)), "public {}%", share(Tier::Public));
    assert!((45..=80).contains(&share(Tier::Internal)), "internal {}%", share(Tier::Internal));
    assert!((12..=40).contains(&share(Tier::Confidential)), "confidential {}%", share(Tier::Confidential));
    assert!((1..=10).contains(&share(Tier::CrownJewels)), "crown jewels {}%", share(Tier::CrownJewels));
    let unlabeled = docs.iter().filter(|d| d.labeled.is_none()).count() * 100 / docs.len();
    assert!(unlabeled > 50, "most documents arrive unclassified: {unlabeled}%");
    assert!(docs.iter().any(|d| d.labeled.is_some_and(|l| l < d.truth)), "some are mislabeled");
    assert!(docs.iter().any(|d| d.sharing.iter().any(|s| s.kind == "anyone")), "some are overshared");
    assert!(work.len() > 100, "{}", work.len());
}

#[test]
fn everything_refers_to_things_that_exist() {
    let (c, all, work) = world(3);
    let ids: HashSet<&str> = all.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids.len(), all.len(), "IDs are unique");
    let humans: HashSet<&str> = c.humans.iter().map(|p| p.user_name.as_str()).collect();
    let accounts: HashSet<&str> = c.service_accounts.iter().map(|s| s.id.as_str()).collect();
    for r in &all {
        assert!(r.parent.as_deref().is_none_or(|p| ids.contains(p)), "{} has a real parent", r.id);
        assert!(humans.contains(r.owner.as_str()), "{} has a real owner", r.id);
    }
    for w in &work {
        assert!(w.resources.iter().all(|r| ids.contains(r.as_str())), "{} links real resources", w.id);
        assert!(w.assignees.iter().all(|a| humans.contains(a.as_str()) || accounts.contains(a.as_str())));
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for w in &work {
        *seen.entry(w.id.as_str()).or_default() += 1;
    }
    assert!(seen.values().all(|n| *n == 1), "work item IDs are unique");
}

/// How awzy-classify does on the generated documents against the true tiers, with each
/// document's text and with metadata only. Printed for `bazel test --test_output=all`.
#[test]
fn the_classifier_against_ground_truth() {
    use awzy_classify::{classify, Facts, Tier as Classified};

    let (_, all, _) = world(1);
    let by_id: HashMap<&str, &Resource> = all.iter().map(|r| (r.id.as_str(), r)).collect();
    let docs: Vec<&Resource> = all.iter().filter(|r| r.kind == "document").collect();
    let tier = |t: Tier| Classified::parse(t.as_str()).unwrap();

    for with_text in [true, false] {
        let (mut exact, mut under, mut over, mut undecided, mut leaked) = (0, 0, 0, 0, 0);
        for d in &docs {
            let mut folders = Vec::new();
            let mut parent = d.parent.as_deref();
            while let Some(f) = parent.and_then(|id| by_id.get(id)) {
                folders.push(f.name.as_str());
                parent = f.parent.as_deref();
            }
            let facts = Facts {
                kind: d.kind,
                name: &d.name,
                sensitivity: d.labeled.map(tier),
                labels: &d.labels,
                folders: &folders,
                text: with_text.then_some(d.text.as_str()),
            };
            let truth = tier(d.truth);
            match classify(&facts).map(|c| c.tier) {
                None => undecided += 1,
                Some(t) if t == truth => exact += 1,
                Some(t) if t < truth => {
                    under += 1;
                    if t == Classified::Public {
                        leaked += 1;
                    }
                }
                Some(_) => over += 1,
            }
        }
        let pct = |n: usize| n * 100 / docs.len();
        println!(
            "classifier, {}: {} documents; exact {}%, under {}%, over {}%, undecided {}%",
            if with_text { "with text" } else { "metadata only" },
            docs.len(),
            pct(exact),
            pct(under),
            pct(over),
            pct(undecided)
        );
        assert_eq!(leaked, 0, "nothing more sensitive than public is ever called public");
        if with_text {
            // Generated text always carries its marking, so this measures the rules, not
            // how messy real documents are (demo plan, "What synthetic data won't tell us").
            assert_eq!((under, undecided), (0, 0), "mislabeled documents are raised to their truth");
            assert!(exact * 100 >= docs.len() * 99, "exact {exact} of {}", docs.len());
        } else {
            assert!(exact + over > 0 && undecided < docs.len(), "metadata places some documents");
        }
    }
}

#[test]
fn the_scenarios_are_set_up_as_the_plan_describes() {
    let (c, all, work) = world(1);
    let find = |id: &str| all.iter().find(|r| r.id == id).unwrap();
    let plan = find("synthetic:doc:strategic-plan-fy27");
    assert_eq!((plan.truth, plan.labeled), (Tier::CrownJewels, Some(Tier::CrownJewels)));
    assert!(plan.sharing.iter().any(|s| s.id == "sales"), "overshared to sales, as the paper's static check allows");
    assert_eq!(plan.to_input()["sensitivity"], "crown_jewels");

    let nora = cast("sales-northeast-finserv").unwrap();
    let noras: Vec<&WorkItem> = work.iter().filter(|w| w.assignees.contains(&nora)).collect();
    assert_eq!(noras.len(), 3, "Nora has three accounts");
    assert!(noras.iter().all(|w| w.assignees.contains(&"sales-genie".to_string())), "with SalesGenie on them");
    assert!(noras.iter().all(|w| !w.resources.contains(&plan.id)), "none of which needs the plan");

    let sam = cast("contractors-quarry-labs").unwrap();
    let sow = work.iter().find(|w| w.kind == "statement_of_work").unwrap();
    assert!(sow.assignees.contains(&sam));
    assert!(
        !sow.resources.contains(&"synthetic:doc:payments-architecture".to_string()),
        "outside the contractors' scope"
    );
    assert!(!sow.resources.contains(&"synthetic:repo:payments-service".to_string()));
    assert!(c.humans.iter().any(|p| p.user_name == sam));

    let runbook = find("synthetic:doc:production-admin-runbook");
    assert!(runbook.sharing.iter().any(|s| s.id == "it-admin"), "IT's own runbook");
    for (id, _, _) in BASIC_QUESTIONS {
        let basic = find(&format!("synthetic:doc:{id}"));
        assert_eq!(basic.truth, Tier::Internal);
        assert!(basic.sharing.iter().any(|s| s.id == "everyone"), "{id} is open to everyone");
    }

    let unlabeled = find("synthetic:doc:00010-000");
    assert!(unlabeled.to_input().get("truth").is_none(), "the truth isn't sent");
}
