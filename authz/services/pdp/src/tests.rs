use super::*;
use awzy_model::Challenges;
use serde_json::json;

/// alice is on security, bob on engineering. Two AI service accounts: sales-genie
/// (engineering, owned by bob) and ops-bot (security, owned by alice).
fn fixtures(pdp: &Pdp) -> Entities {
    pdp.entities(json!([
        { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
        { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
        { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] },
        { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
          "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true },
          "parents": [{ "type": "Group", "id": "engineering" }] },
        { "uid": { "type": "ServiceAccount", "id": "ops-bot" },
          "attrs": { "owner": { "__entity": { "type": "Human", "id": "alice" } }, "ai": true },
          "parents": [{ "type": "Group", "id": "security" }] },
        { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
        { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
    ]))
    .unwrap()
}

fn read(who: &str, what: &str, risk: i64) -> Check {
    Check {
        principal: EntityRef::new("Human", who),
        action: "read".into(),
        resource: EntityRef::new("Resource", what),
        context: json!({ "risk_score": risk }).as_object().unwrap().clone(),
        on_behalf_of: None,
        human_present: None,
        always_enforced: false,
    }
}

/// `account` reading `what`, optionally acting for the person `for_human`.
fn sa_read(account: &str, for_human: Option<&str>, what: &str, risk: i64) -> Check {
    Check {
        principal: EntityRef::new("ServiceAccount", account),
        on_behalf_of: for_human.map(|h| EntityRef::new("Human", h)),
        ..read(account, what, risk)
    }
}

fn decide(req: &Check) -> Decision {
    let pdp = Pdp::with_defaults().unwrap();
    pdp.decide(req, &fixtures(&pdp)).unwrap()
}

#[test]
fn a_bundle_published_before_unclassified_still_loads_resources() {
    let old_schema = DEFAULT_SCHEMA.replace("unclassified?: Bool,", "");
    assert_ne!(old_schema, DEFAULT_SCHEMA);
    let pdp = Pdp::new(&old_schema, DEFAULT_POLICIES).unwrap();
    let resource = json!([{ "uid": { "type": "Resource", "id": "doc" },
        "attrs": { "sensitivity": "internal", "unclassified": true }, "parents": [] }]);
    assert!(pdp.entities(resource.clone()).is_ok(), "the attribute is dropped, not an error");
    assert!(Pdp::with_defaults().unwrap().entities(resource).is_ok());
}

#[test]
fn assignment_is_checked_for_the_agent_and_the_person_it_acts_for() {
    let policies = r#"
        @id("anyone-reads")
        permit (principal, action == Action::"read", resource);
        @id("crown-jewels-need-assigned-work")
        @challenge("approval")
        forbid (principal, action == Action::"read", resource)
        when { resource.sensitivity == "crown_jewels" && context has assigned && !context.assigned };
    "#;
    let pdp = Pdp::new(DEFAULT_SCHEMA, policies).unwrap();
    let entities = pdp
        .entities(json!([
            { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
              "attrs": { "owner": { "__entity": { "type": "Human", "id": "nora" } }, "ai": true }, "parents": [] },
            { "uid": { "type": "Resource", "id": "plan" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
        ]))
        .unwrap();
    let req = Check { human_present: Some(true), ..sa_read("sales-genie", Some("nora"), "plan", 1) };
    let assigned = |who: &[&str]| who.iter().map(|w| w.to_string()).collect::<HashSet<String>>();
    let challenged = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) };

    // The rogue agent: neither SalesGenie nor Nora has work that needs the plan.
    let d = pdp.decide_assigned(&req, &entities, Some(&assigned(&["strategy-lead"]))).unwrap();
    assert_eq!((d.verdict, d.on_behalf_of.unwrap().verdict), (challenged, challenged));

    // The agent alone being assigned isn't enough: the person's check still needs it.
    let d = pdp.decide_assigned(&req, &entities, Some(&assigned(&["sales-genie"]))).unwrap();
    assert_eq!(d.policies, vec!["anyone-reads"], "the account's own check passed");
    assert_eq!(d.verdict, challenged);

    let both = assigned(&["sales-genie", "nora"]);
    assert_eq!(pdp.decide_assigned(&req, &entities, Some(&both)).unwrap().verdict, Verdict::Allow);
    // Unknown (no awzy-context): the policy can't tell, and doesn't fire.
    assert_eq!(pdp.decide(&req, &entities).unwrap().verdict, Verdict::Allow);

    // A bundle published before `assigned` existed still decides.
    let old = Pdp::new(&DEFAULT_SCHEMA.replace("assigned?: Bool,", ""), DEFAULT_POLICIES).unwrap();
    let entities = old.entities(json!([{ "uid": { "type": "Resource", "id": "plan" }, "attrs": { "sensitivity": "internal" }, "parents": [] }])).unwrap();
    assert!(old.decide_assigned(&read("bob", "plan", 1), &entities, Some(&both)).is_ok());
}

#[test]
fn policy_sources_by_id() {
    let found = policy_sources(DEFAULT_POLICIES, &["high-risk-verify".into()]).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "high-risk-verify");
    assert!(found[0].1.contains("context.risk_score > 80"), "{}", found[0].1);
}

#[test]
fn security_team_reads_anything() {
    let d = decide(&read("alice", "board-deck", 10));
    assert_eq!(d.verdict, Verdict::Allow);
    assert_eq!(d.policies, vec!["security-team-read"]);
    assert!(d.on_behalf_of.is_none());
}

#[test]
fn a_passed_challenge_lifts_only_the_policies_that_asked_for_it() {
    let pdp = Pdp::with_defaults().unwrap();
    let entities = fixtures(&pdp);
    let passed = |ids: &[&str]| ids.iter().map(|i| i.to_string()).collect::<HashSet<_>>();
    let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };

    let challenged = pdp.decide(&read("bob", "design-doc", 90), &entities).unwrap();
    assert_eq!(pdp.asking_policies(&challenged), [("high-risk-verify".to_string(), ChallengeKind::Verification)]);
    let d = pdp.decide_passed(&read("bob", "design-doc", 90), &entities, None, &passed(&["high-risk-verify"]));
    let d = d.unwrap();
    assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["engineering-read-internal".to_string()]));

    let other = passed(&["absent-human-approval"]);
    let d = pdp.decide_passed(&read("bob", "design-doc", 90), &entities, None, &other).unwrap();
    assert_eq!(d.verdict, verify, "an answer to another policy isn't one to this");

    // A challenge sits on top of a permit; passing one never grants what no policy permits.
    let d = pdp.decide_passed(&read("bob", "board-deck", 90), &entities, None, &passed(&["high-risk-verify"]));
    assert_eq!(d.unwrap().verdict, Verdict::Deny);
    // Nor does it lift a plain forbid named as if it had asked.
    let deactivated = pdp
        .entities(json!([
            { "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": false }, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
        ]))
        .unwrap();
    let d = pdp.decide_passed(&read("bob", "design-doc", 10), &deactivated, None, &passed(&["deactivated-humans"]));
    assert_eq!(d.unwrap().verdict, Verdict::Deny);
}

#[test]
fn the_mcp_client_reaches_policy_and_a_schema_without_it_still_decides() {
    let mut req = read("bob", "design-doc", 10);
    req.context.insert("mcp_client".into(), json!("claude-code"));

    let forbid = format!(
        "{DEFAULT_POLICIES}\n@id(\"no-claude-code\")\nforbid (principal, action, resource) \
         when {{ context has mcp_client && context.mcp_client == \"claude-code\" }};"
    );
    let pdp = Pdp::new(DEFAULT_SCHEMA, &forbid).unwrap();
    let d = pdp.decide(&req, &fixtures(&pdp)).unwrap();
    assert_eq!((d.verdict, d.policies), (Verdict::Deny, vec!["no-claude-code".to_string()]));

    let older = DEFAULT_SCHEMA.replace("  mcp_client?: String,\n", "");
    assert_ne!(older, DEFAULT_SCHEMA, "the schema declares it");
    let pdp = Pdp::new(&older, DEFAULT_POLICIES).unwrap();
    assert_eq!(pdp.decide(&req, &fixtures(&pdp)).unwrap().verdict, Verdict::Allow, "dropped, not refused");
}

#[test]
fn a_person_scored_as_software_touches_their_key_for_sensitive_data() {
    let pdp = Pdp::with_defaults().unwrap();
    let entities = fixtures(&pdp);
    let automation =
        |score: i64| -> std::collections::HashMap<String, i64> { [("Human::alice".to_string(), score)].into() };
    let decide = |what: &str, score: i64| {
        let scores = automation(score);
        let facts = Facts { automation: Some(&scores), ..Facts::default() };
        pdp.decide_with(&read("alice", what, 10), &entities, &facts, &HashSet::new()).unwrap()
    };
    let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };
    let d = decide("board-deck", 95);
    assert_eq!((d.verdict, d.policies), (verify, vec!["automated-people-verify".to_string()]));
    assert_eq!(decide("board-deck", 30).verdict, Verdict::Allow, "a person at the keyboard");
    assert_eq!(decide("design-doc", 95).verdict, Verdict::Allow, "internal data asks nothing");

    let older = DEFAULT_SCHEMA.replace("  automation_score?: Long,\n", "");
    assert_ne!(older, DEFAULT_SCHEMA);
    assert!(DEFAULT_POLICIES.contains(AUTOMATED_POLICY));
    let without = DEFAULT_POLICIES.replace(AUTOMATED_POLICY, "");
    let pdp = Pdp::new(&older, &without).unwrap();
    let scores = automation(95);
    let facts = Facts { automation: Some(&scores), ..Facts::default() };
    let d = pdp.decide_with(&read("alice", "board-deck", 10), &fixtures(&pdp), &facts, &HashSet::new());
    assert_eq!(d.unwrap().verdict, Verdict::Allow, "a schema without it still decides");
}

/// The base policy that reads `automation_score`, for a bundle whose schema predates it.
const AUTOMATED_POLICY: &str = "@id(\"automated-people-verify\")\n@challenge(\"verification\")\nforbid (\n  principal is Human,\n  action in [Action::\"read\", Action::\"write\", Action::\"call\"],\n  resource\n)\nwhen {\n  context has automation_score && context.automation_score > 80 && resource has sensitivity &&\n  (resource.sensitivity == \"confidential\" || resource.sensitivity == \"crown_jewels\")\n};\n";

#[test]
fn switching_enforcement_takes_the_security_teams_key_and_never_an_agent() {
    let pdp = Pdp::with_defaults().unwrap();
    let switch = |check: Check| Check {
        action: "switch_enforcement".into(),
        resource: EntityRef::new("PortalArea", "enforcement"),
        ..check
    };
    let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };
    let d = pdp.decide(&switch(read("alice", "", 10)), &fixtures(&pdp)).unwrap();
    assert_eq!((d.verdict, d.policies), (verify, vec!["enforcement-switches-need-a-key".to_string()]));
    assert_eq!(pdp.decide(&switch(read("bob", "", 10)), &fixtures(&pdp)).unwrap().verdict, Verdict::Deny);
    let agent = switch(sa_read("ops-bot", Some("alice"), "", 10));
    assert_eq!(pdp.decide(&agent, &fixtures(&pdp)).unwrap().verdict, Verdict::Deny, "even acting for alice");
}

/// A support session's check (ADR 0095): what the client portal sends for an awzy staff member
/// whose session approved only the Decisions page.
fn support(action: &str, area: &str) -> Check {
    let mut req = Check {
        action: action.into(),
        resource: EntityRef::new("PortalArea", area),
        ..read("awzy-support:max", "", 10)
    };
    req.context.insert("support_areas".into(), json!([{ "__entity": { "type": "PortalArea", "id": "decisions" } }]));
    req
}

#[test]
fn a_support_session_views_only_the_areas_approved_and_changes_nothing() {
    let pdp = Pdp::with_defaults().unwrap();
    let verdict = |c: Check| pdp.decide(&c, &fixtures(&pdp)).unwrap().verdict;
    assert_eq!(verdict(support("view", "decisions")), Verdict::Allow);
    assert_eq!(verdict(support("view", "challenges")), Verdict::Deny, "not even what every person may view");
    assert_eq!(verdict(support("administer", "decisions")), Verdict::Deny);
    assert_eq!(verdict(support("switch_enforcement", "enforcement")), Verdict::Deny);
}

#[test]
fn a_bundle_from_before_support_sessions_denies_them() {
    let older = DEFAULT_SCHEMA.replace("  support_areas?: Set<PortalArea>,\n", "");
    assert_ne!(older, DEFAULT_SCHEMA);
    let start = DEFAULT_POLICIES.find("// Support sessions (ADR 0095)").unwrap();
    let end = DEFAULT_POLICIES.find("// Everyone answers the challenges").unwrap();
    let without = format!("{}{}", &DEFAULT_POLICIES[..start], &DEFAULT_POLICIES[end..]);
    let pdp = Pdp::new(&older, &without).unwrap();
    assert_eq!(pdp.decide(&support("view", "decisions"), &fixtures(&pdp)).unwrap().verdict, Verdict::Deny);
}

#[test]
fn the_security_team_approves_challenges() {
    let pdp = Pdp::with_defaults().unwrap();
    let approve = |who: &str| Check { action: "approve".into(), ..read(who, "board-deck", 10) };
    assert_eq!(pdp.decide(&approve("alice"), &fixtures(&pdp)).unwrap().verdict, Verdict::Allow);
    assert_eq!(pdp.decide(&approve("bob"), &fixtures(&pdp)).unwrap().verdict, Verdict::Deny);
}

#[test]
fn an_agent_approves_only_where_a_policy_names_it_and_never_for_its_owner() {
    let approve = |agent: &str, what: &str, approving_for: Option<&str>| {
        let mut req = Check { action: "approve".into(), ..sa_read(agent, None, what, 10) };
        if let Some(h) = approving_for {
            req.context.insert("approving_for".into(), json!({ "__entity": { "type": "Human", "id": h } }));
        }
        req
    };
    // ops-bot is in the security group, but the base policies let only its people approve.
    let base = Pdp::with_defaults().unwrap();
    assert_eq!(
        base.decide(&approve("ops-bot", "design-doc", Some("bob")), &fixtures(&base)).unwrap().verdict,
        Verdict::Deny
    );

    let overseer = format!(
        "{DEFAULT_POLICIES}\n@id(\"ops-bot-approves\") permit (principal == ServiceAccount::\"ops-bot\", action == Action::\"approve\", resource);"
    );
    let pdp = Pdp::new(DEFAULT_SCHEMA, &overseer).unwrap();
    let verdict = |req| pdp.decide(&req, &fixtures(&pdp)).unwrap().verdict;
    assert_eq!(verdict(approve("ops-bot", "design-doc", Some("bob"))), Verdict::Allow);
    assert_eq!(verdict(approve("ops-bot", "design-doc", Some("alice"))), Verdict::Deny, "alice owns ops-bot");
    assert_eq!(verdict(approve("ops-bot", "design-doc", None)), Verdict::Deny, "no telling whose it is");
    assert_eq!(verdict(approve("ops-bot", "board-deck", Some("bob"))), Verdict::Deny, "crown jewels need a person");
}

#[test]
fn engineering_cannot_read_crown_jewels() {
    assert_eq!(decide(&read("bob", "board-deck", 10)).verdict, Verdict::Deny);
}

#[test]
fn high_risk_turns_allow_into_challenge() {
    let d = decide(&read("bob", "design-doc", 90));
    assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) });
    assert_eq!(d.policies, vec!["high-risk-verify"]);
}

#[test]
fn an_administrator_at_elevated_risk_touches_a_key_for_high_risk_operations() {
    let pdp = Pdp::with_defaults().unwrap();
    let entities = pdp
        .entities(json!([
            { "uid": { "type": "Group", "id": "it" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Group", "id": "it-admin" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it" }] },
            { "uid": { "type": "Human", "id": "lee" }, "attrs": {}, "parents": [{ "type": "Group", "id": "it-admin" }] },
            { "uid": { "type": "McpTool", "id": "larkspur-admin/export_directory" },
              "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
            { "uid": { "type": "McpTool", "id": "larkspur-admin/reset_password" },
              "attrs": { "sensitivity": "confidential" }, "parents": [] },
        ]))
        .unwrap();
    let call = |tool: &str, risk: i64| Check {
        action: "call".into(),
        resource: EntityRef::new("McpTool", format!("larkspur-admin/{tool}")),
        ..read("lee", "", risk)
    };
    let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };

    // Routine for IT at ordinary risk.
    let d = pdp.decide(&call("export_directory", 10), &entities).unwrap();
    assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["it-calls-any-tool".to_string()]));
    // Elevated: the high-risk operation needs the key, the ordinary one doesn't.
    let d = pdp.decide(&call("export_directory", 50), &entities).unwrap();
    assert_eq!((d.verdict, d.policies), (verify, vec!["elevated-risk-high-risk-operations-verify".to_string()]));
    assert_eq!(pdp.decide(&call("reset_password", 50), &entities).unwrap().verdict, Verdict::Allow);
    // And touching it is what lets the operation run.
    let passed: HashSet<String> = ["elevated-risk-high-risk-operations-verify".to_string()].into();
    let d = pdp.decide_passed(&call("export_directory", 50), &entities, None, &passed).unwrap();
    assert_eq!(d.verdict, Verdict::Allow);
    // Risk rose past 80 since: high-risk-verify asks too, and that touch wasn't for it.
    let d = pdp.decide_passed(&call("export_directory", 90), &entities, None, &passed).unwrap();
    assert_eq!((d.verdict, d.policies), (verify, vec!["high-risk-verify".to_string()]));
}

/// Larkspur's shapes (ADR 0061): Sam, a Quarry Labs contractor in engineering; Nora in sales;
/// SalesGenie in Nora's team; Priya owns the payments design and the strategic plan.
fn larkspur(pdp: &Pdp) -> Entities {
    let group = |id: &str| json!({ "__entity": { "type": "Group", "id": id } });
    let human = |id: &str| json!({ "__entity": { "type": "Human", "id": id } });
    let doc = |id: &str, tier: &str, owner: &str, groups: &[&str], writers: &[&str]| {
        json!({ "uid": { "type": "Resource", "id": id }, "parents": [], "attrs": {
            "sensitivity": tier, "owner": human(owner),
            "shared_humans": [], "shared_groups": groups.iter().map(|g| group(g)).collect::<Vec<_>>(),
            "writer_humans": [], "writer_groups": writers.iter().map(|g| group(g)).collect::<Vec<_>>(),
            "shared_with_everyone": false } })
    };
    pdp.entities(json!([
        { "uid": { "type": "Human", "id": "sam" }, "attrs": {}, "parents": [{ "type": "Group", "id": "contractors-quarry-labs" }] },
        { "uid": { "type": "Group", "id": "contractors-quarry-labs" }, "attrs": {},
          "parents": [{ "type": "Group", "id": "engineering" }, { "type": "Group", "id": "contractors" }] },
        { "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Group", "id": "contractors" }, "attrs": {}, "parents": [] },
        { "uid": { "type": "Human", "id": "nora" }, "attrs": {}, "parents": [{ "type": "Group", "id": "sales-ne" }] },
        { "uid": { "type": "Group", "id": "sales-ne" }, "attrs": {}, "parents": [{ "type": "Group", "id": "sales" }] },
        { "uid": { "type": "Group", "id": "sales" }, "attrs": {}, "parents": [] },
        { "uid": { "type": "ServiceAccount", "id": "sales-genie" },
          "attrs": { "owner": human("nora"), "ai": true }, "parents": [{ "type": "Group", "id": "sales-ne" }] },
        { "uid": { "type": "Human", "id": "priya" }, "attrs": {}, "parents": [] },
        doc("payments", "confidential", "priya", &["engineering"], &[]),
        doc("plan", "crown_jewels", "priya", &["sales"], &[]),
        doc("export", "confidential", "priya", &["finance"], &[]),
        doc("accounts", "internal", "nora", &["sales-ne"], &["sales-ne"]),
    ]))
    .unwrap()
}

fn as_(who: &str, kind: &str, action: &str, what: &str) -> Check {
    Check { principal: EntityRef::new(kind, who), action: action.into(), ..read(who, what, 0) }
}

#[test]
fn owners_and_shares_grant_reads_and_writers_grant_writes() {
    let pdp = Pdp::with_defaults().unwrap();
    let e = larkspur(&pdp);
    let nobody_assigned = HashSet::new();
    let decide = |r: Check| pdp.decide_assigned(&r, &e, Some(&nobody_assigned)).unwrap();
    assert_eq!(decide(as_("nora", "Human", "read", "accounts")).verdict, Verdict::Allow, "shared with her team");
    assert_eq!(decide(as_("nora", "Human", "write", "accounts")).verdict, Verdict::Allow, "her team writes");
    assert_eq!(decide(as_("nora", "Human", "read", "export")).verdict, Verdict::Deny, "not shared with her");
    assert_eq!(decide(as_("sam", "Human", "write", "payments")).verdict, Verdict::Deny, "engineering only reads");
    assert_eq!(decide(as_("priya", "Human", "write", "export")).verdict, Verdict::Allow, "the owner writes");
    assert_eq!(decide(as_("priya", "Human", "approve", "payments")).verdict, Verdict::Allow, "owners approve");
    assert_eq!(decide(as_("nora", "Human", "approve", "payments")).verdict, Verdict::Deny);
}

#[test]
fn the_curious_contractor_needs_the_owners_approval() {
    let pdp = Pdp::with_defaults().unwrap();
    let e = larkspur(&pdp);
    let d = pdp.decide_assigned(&as_("sam", "Human", "read", "payments"), &e, Some(&HashSet::new())).unwrap();
    assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) });
    assert_eq!(d.policies, vec!["contractors-outside-their-work"]);
    let mine = HashSet::from(["sam".to_string()]);
    let d = pdp.decide_assigned(&as_("sam", "Human", "read", "payments"), &e, Some(&mine)).unwrap();
    assert_eq!(d.verdict, Verdict::Allow, "within his statement of work");
    let d = pdp.decide_assigned(&as_("sam", "Human", "read", "export"), &e, Some(&HashSet::new())).unwrap();
    assert_eq!(d.verdict, Verdict::Deny, "not shared with him at all: no challenge to pass");
}

#[test]
fn the_rogue_agent_needs_its_person_and_the_owner() {
    let pdp = Pdp::with_defaults().unwrap();
    let e = larkspur(&pdp);
    let genie = Check {
        on_behalf_of: Some(EntityRef::new("Human", "nora")),
        human_present: Some(true),
        ..as_("sales-genie", "ServiceAccount", "read", "plan")
    };
    let d = pdp.decide_assigned(&genie, &e, Some(&HashSet::new())).unwrap();
    let both = [ChallengeKind::Justification, ChallengeKind::Approval].into_iter().collect();
    assert_eq!(d.verdict, Verdict::Challenge { challenges: both });
    assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Allow, "Nora herself may read the overshared plan");
    let nora = Check { on_behalf_of: None, ..as_("nora", "Human", "read", "plan") };
    assert_eq!(pdp.decide_assigned(&nora, &e, Some(&HashSet::new())).unwrap().verdict, Verdict::Allow);
    let accounts = Check { resource: EntityRef::new("Resource", "accounts"), ..genie };
    assert_eq!(pdp.decide_assigned(&accounts, &e, Some(&HashSet::new())).unwrap().verdict, Verdict::Allow, "internal");
}

#[test]
fn a_challenge_only_stands_in_front_of_something_policy_allows() {
    // Risk over 80 asks for a security key, but nothing permits bob to read the board deck:
    // passing the challenge would still end in a deny, so it's a deny now.
    let d = decide(&read("bob", "board-deck", 90));
    assert_eq!(d.verdict, Verdict::Deny);
    assert_eq!(decide(&read("bob", "design-doc", 90)).verdict.as_str(), "challenge", "permitted, then challenged");
}

#[test]
fn a_bundle_from_before_sharing_still_loads_its_resources() {
    let mut old_schema = DEFAULT_SCHEMA.to_string();
    let start = old_schema.find("  // Who owns it in its source system").unwrap();
    let end = old_schema[start..].find("};").unwrap() + start;
    old_schema.replace_range(start..end, "");
    let policies = "permit (principal, action == Action::\"read\", resource);";
    let pdp = Pdp::new(&old_schema, policies).unwrap();
    assert!(pdp
        .entities(json!([{ "uid": { "type": "Resource", "id": "doc" }, "parents": [], "attrs": {
        "sensitivity": "internal", "owner": { "__entity": { "type": "Human", "id": "p" } },
        "shared_groups": [], "shared_with_everyone": true } }]))
        .is_ok());
}

#[test]
fn delegation_never_exceeds_the_person() {
    // ops-bot (security) could read the board deck itself; bob can't, so acting for bob it can't.
    let d = decide(&sa_read("ops-bot", Some("bob"), "board-deck", 10));
    assert_eq!(d.verdict, Verdict::Deny);
    assert_eq!(d.policies, vec!["security-team-read"], "the account's own check allowed");
    assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Deny);
}

#[test]
fn delegation_never_exceeds_the_account() {
    // alice could read the board deck; sales-genie (engineering) can't, even for her.
    let d = decide(&sa_read("sales-genie", Some("alice"), "board-deck", 10));
    assert_eq!(d.verdict, Verdict::Deny);
    let human = d.on_behalf_of.unwrap();
    assert_eq!((human.verdict, human.policies), (Verdict::Allow, vec!["security-team-read".to_string()]));
}

#[test]
fn unattended_ai_never_reads_crown_jewels() {
    let d = decide(&sa_read("ops-bot", None, "board-deck", 10));
    assert_eq!(d.verdict, Verdict::Deny);
    assert_eq!(d.policies, vec!["unattended-ai-no-crown-jewels"]);
}

#[test]
fn absent_person_needs_approval_when_risky() {
    let mut req = sa_read("sales-genie", Some("bob"), "design-doc", 60);
    let d = decide(&req);
    assert_eq!(d.verdict, Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Approval) });
    assert_eq!(d.policies, vec!["absent-human-approval"]);
    assert_eq!(d.on_behalf_of.unwrap().verdict, Verdict::Allow);

    req.human_present = Some(true);
    assert_eq!(decide(&req).verdict, Verdict::Allow, "a person present can answer for it");
}

#[test]
fn every_challenge_that_fires_is_required() {
    // Risk 90 with no person present: absent-human-approval and high-risk-verify both fire.
    let d = decide(&sa_read("sales-genie", Some("bob"), "design-doc", 90));
    let all = [ChallengeKind::Verification, ChallengeKind::Approval].into_iter().collect();
    assert_eq!(d.verdict, Verdict::Challenge { challenges: all });
    assert_eq!(d.policies, vec!["absent-human-approval", "high-risk-verify"]);
}

#[test]
fn callers_cannot_set_delegation_in_the_context() {
    let pdp = Pdp::with_defaults().unwrap();
    for key in ["on_behalf_of", "human_present", "assigned", "automation_score"] {
        let mut req = read("bob", "design-doc", 10);
        req.context.insert(key.into(), json!(true));
        let err = pdp.decide(&req, &fixtures(&pdp)).unwrap_err();
        assert!(err.to_string().contains("is set by the PDP"), "{err}");
    }
}

#[tokio::test]
async fn the_idps_groups_and_deactivation_win_over_the_token() {
    let principals = Arc::new(principals::Principals::awaiting_sync());
    let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_principals(principals.clone());
    // What the enforcement points send, from tokens: bob in engineering, alice in security.
    let entities = json!([
        { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
        { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
        { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
    ]);
    assert!(
        matches!(state.check(&read("bob", "board-deck", 1), entities.clone()).await, Err(CheckError::NotReady)),
        "waits for awzy-context"
    );

    // The IdP has since moved bob into security and deactivated alice.
    principals.load(
        1,
        vec![
            json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "security" }] }),
            json!({ "uid": { "type": "Human", "id": "alice" }, "attrs": { "active": false }, "parents": [] }),
            json!({ "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] }),
        ],
    );
    let bob = state.check(&read("bob", "board-deck", 1), entities.clone()).await.unwrap();
    assert_eq!((bob.verdict, bob.policies.clone()), (Verdict::Allow, vec!["security-team-read".to_string()]));
    let alice = state.check(&read("alice", "board-deck", 1), entities).await.unwrap();
    assert_eq!((alice.verdict, alice.policies), (Verdict::Deny, vec!["deactivated-humans".to_string()]));

    // A service account awzy-context doesn't know is denied, whatever the request says.
    let rogue = json!([{ "uid": { "type": "ServiceAccount", "id": "rogue-bot" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] }]);
    let d = state.check(&sa_read("rogue-bot", None, "board-deck", 1), rogue).await.unwrap();
    assert_eq!((d.verdict, d.policies), (Verdict::Deny, vec![principals::UNREGISTERED.to_string()]));
}

#[tokio::test]
async fn check_stamps_the_mode_and_waits_for_the_first_sync() {
    let entities = json!([
        { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
        { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "crown_jewels" }, "parents": [] },
    ]);
    let enforcement = Arc::new(Enforcement::awaiting_sync());
    let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_enforcement(enforcement.clone());
    assert!(matches!(state.check(&read("bob", "board-deck", 1), entities.clone()).await, Err(CheckError::NotReady)));

    enforcement.synced(EnforcementMode::Monitor);
    let d = state.check(&read("bob", "board-deck", 1), entities.clone()).await.unwrap();
    assert_eq!((d.verdict, d.mode), (Verdict::Deny, EnforcementMode::Monitor), "a would-deny");
    let portal = Check { always_enforced: true, ..read("bob", "board-deck", 1) };
    assert_eq!(
        state.check(&portal, entities.clone()).await.unwrap().mode,
        EnforcementMode::Enforce,
        "applied whatever the mode"
    );

    enforcement.synced(EnforcementMode::Enforce);
    assert_eq!(state.check(&read("bob", "board-deck", 1), entities).await.unwrap().mode, EnforcementMode::Enforce);
}

#[tokio::test]
async fn each_check_sees_its_own_principals_risk_score() {
    let principals = Arc::new(principals::Principals::awaiting_sync());
    principals.load_with_risk(
        1,
        vec![
            json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": { "active": true }, "parents": [{ "type": "Group", "id": "engineering" }] }),
            json!({ "uid": { "type": "Group", "id": "engineering" }, "attrs": {}, "parents": [] }),
            json!({ "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                    "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true },
                    "parents": [{ "type": "Group", "id": "engineering" }] }),
        ],
        [("Human::bob".to_string(), 90), ("ServiceAccount::sales-genie".to_string(), 10)].into(),
    );
    let state = AppState::new(shared(Pdp::with_defaults().unwrap())).with_principals(principals);
    let doc = json!([{ "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
    let verify = Verdict::Challenge { challenges: Challenges::of(ChallengeKind::Verification) };

    // The enforcement point says 0; awzy's score for bob is 90, and high-risk-verify fires.
    let d = state.check(&read("bob", "design-doc", 0), doc.clone()).await.unwrap();
    assert_eq!((d.verdict, d.policies), (verify, vec!["high-risk-verify".to_string()]));

    // SalesGenie (10) acting for bob (90): its own check passes, bob's is challenged, and the
    // stricter verdict stands.
    let req = Check { human_present: Some(true), ..sa_read("sales-genie", Some("bob"), "design-doc", 0) };
    let d = state.check(&req, doc.clone()).await.unwrap();
    assert_eq!(d.verdict, verify);
    assert_eq!(d.on_behalf_of.unwrap().verdict, verify);
    assert!(!d.policies.contains(&"high-risk-verify".to_string()), "the agent's own score is low");

    // Someone awzy hasn't scored keeps what the enforcement point sent, as before.
    let carol = json!([{ "uid": { "type": "Human", "id": "carol" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
                      { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
    assert_eq!(state.check(&read("carol", "design-doc", 5), carol).await.unwrap().verdict, Verdict::Allow);
}

#[tokio::test]
async fn resources_are_decided_on_what_awzy_context_says() {
    use axum::{extract::Path, routing::get};

    // awzy-context knows the board deck is crown jewels; nothing else.
    let app = Router::new().route(
        "/v1/resources/{id}/entities",
        get(|Path(id): Path<String>| async move {
            if id != "board-deck" {
                return Err(StatusCode::NOT_FOUND);
            }
            Ok(Json(json!({ "entities": [{ "uid": { "type": "Resource", "id": "board-deck" },
                "attrs": { "sensitivity": "crown_jewels", "unclassified": false }, "parents": [] }] })))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let state = AppState::new(shared(Pdp::with_defaults().unwrap()))
        .with_resources(Arc::new(resources::Resources::from_context(url)));
    let engineer = json!({ "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] });
    let bob = || engineer.clone();

    // The enforcement point claims it's internal; awzy-context's tier decides.
    let claimed = json!([bob(), { "uid": { "type": "Resource", "id": "board-deck" }, "attrs": { "sensitivity": "internal" }, "parents": [] }]);
    assert_eq!(state.check(&read("bob", "board-deck", 1), claimed).await.unwrap().verdict, Verdict::Deny);

    // Only an ID, for a resource awzy-context doesn't know: the default tier.
    let d = state.check(&read("bob", "not-synced-yet", 1), json!([bob()])).await.unwrap();
    assert_eq!((d.verdict, d.policies), (Verdict::Allow, vec!["engineering-read-internal".to_string()]));

    let down = AppState::new(shared(Pdp::with_defaults().unwrap()))
        .with_resources(Arc::new(resources::Resources::from_context("http://127.0.0.1:1".into())));
    assert!(matches!(down.check(&read("bob", "board-deck", 1), json!([bob()])).await, Err(CheckError::Unavailable(_))));
}

#[tokio::test]
async fn http_evaluation() {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    let body = json!({
        "principal": { "type": "Human", "id": "bob" },
        "action": "read",
        "resource": { "type": "Resource", "id": "design-doc" },
        "context": { "risk_score": 5 },
        "entities": [
            { "uid": { "type": "Human", "id": "bob" }, "attrs": {}, "parents": [{ "type": "Group", "id": "engineering" }] },
            { "uid": { "type": "Resource", "id": "design-doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
        ],
    });
    let res = router(AppState::new(shared(Pdp::with_defaults().unwrap())))
        .oneshot(
            Request::post("/v1/access/evaluation")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let decision: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decision["verdict"], "allow");
    assert_eq!(decision["mode"], "monitor");
}

/// A bundle can add an app-owned type (ADR 0108), and a check on it is decided on the attributes
/// the enforcement point sent: no awzy-context lookup, no `unclassified`.
#[test]
fn an_app_owned_type_is_decided_on_the_enforcement_points_attributes() {
    let schema = format!(
        "{DEFAULT_SCHEMA}\n@awzy(\"app\")\nentity Customer {{ tenant: String }};\n\
         action view_customer appliesTo {{ principal: [Human, ServiceAccount], resource: [Customer], context: RequestContext }};"
    );
    let policies = format!(
        "{DEFAULT_POLICIES}\n@id(\"acme-support-views-acme\")\n\
         permit (principal in Group::\"security\", action == Action::\"view_customer\", resource is Customer)\n\
         when {{ resource.tenant == \"acme\" }};"
    );
    let pdp = Pdp::new(&schema, &policies).unwrap();
    assert!(pdp.is_app_type("Customer") && !pdp.is_app_type("Resource"));
    let decide = |tenant: &str| {
        let entities = pdp
            .entities(json!([
                { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
                { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
                { "uid": { "type": "Customer", "id": "c-1" }, "attrs": { "tenant": tenant }, "parents": [] },
            ]))
            .unwrap();
        let check = Check {
            principal: EntityRef::new("Human", "alice"),
            action: "view_customer".into(),
            resource: EntityRef::new("Customer", "c-1"),
            context: json!({ "risk_score": 0 }).as_object().unwrap().clone(),
            on_behalf_of: None,
            human_present: None,
            always_enforced: false,
        };
        pdp.decide(&check, &entities).unwrap()
    };
    let allowed = decide("acme");
    assert_eq!((allowed.verdict, allowed.policies), (Verdict::Allow, vec!["acme-support-views-acme".to_string()]));
    assert_eq!(decide("globex").verdict, Verdict::Deny);
}

/// A bundle that adds a type without marking it app-owned is refused when it's validated, as
/// awzy-policy does on publishing.
#[test]
fn an_unmarked_added_type_fails_validation() {
    let schema = format!("{DEFAULT_SCHEMA}\nentity Customer;");
    let err = validate(&schema, DEFAULT_POLICIES).unwrap_err().to_string();
    assert!(err.contains("mark it @awzy(\"app\")"), "{err}");
}
