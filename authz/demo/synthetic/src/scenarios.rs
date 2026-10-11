//! The demo plan's scenarios: the curious contractor and the first half of the rogue agent, which
//! challenges answer (step 6), and the suddenly foolish administrator and the peer outlier, which
//! need awzy-reasoner's risk too (step 7). Each is a few accesses at fixed times on top of an
//! ordinary day, with what awzy should do about each, so detection can be scored (demo plan,
//! "Detection").
//!
//! The rogue agent's ending also needs awzy-challenge (the owner's rejection) and awzy-intake (the
//! export, [`exfiltration`]).

use std::collections::BTreeSet;

use crate::{
    activity::{Access, Why, LIST, READ},
    driver::Outcome,
    org::{cast, Company, Profile},
    world::{human_documents, work_items, BASIC_QUESTIONS, OUTSIDE_HER_ACCOUNTS},
};

/// When the scenarios count on awzy-reasoner having scored the day so far: after the foolish
/// administrator's questions and the peer outlier's morning, before what they do next.
pub const REASONER_RUNS_AT: u32 = 10 * 60 + 45;

/// What awzy should do with a scenario's access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Challenge it, with these kinds.
    Challenge(&'static [&'static str]),
    /// Deny it outright: nothing permits it.
    Deny,
    /// Let it through: on its own, it's ordinary.
    Allow,
}

impl Expect {
    /// Whether the gateway's outcome is what was expected. The gateway reports the verdict, not
    /// the challenge's kinds; the end-to-end test checks those at the PDP.
    pub fn met_by(self, outcome: &Outcome) -> bool {
        match (self, outcome) {
            (Expect::Challenge(_), Outcome::Blocked(v)) => v == "challenge",
            (Expect::Deny, Outcome::Blocked(v)) => v == "deny",
            (Expect::Allow, Outcome::Ran) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub scenario: &'static str,
    pub access: Access,
    pub expect: Expect,
}

/// Stolen credentials of Sam, a Quarry Labs contractor, used at night on what his statement of
/// work doesn't cover: the payments design, shared with all of engineering (an approval from its
/// owner), and the client holdings export, not shared with him at all (a deny).
pub fn curious_contractor() -> Vec<Step> {
    let sam = cast("contractors-quarry-labs").expect("cast");
    let step = |minute: u32, resource: &str, expect: Expect| Step {
        scenario: "curious-contractor",
        access: Access {
            minute,
            subject: sam.clone(),
            on_behalf_of: None,
            human_present: true,
            team: "contractors-quarry-labs",
            tool: READ,
            resource: resource.into(),
            why: Why::Scenario("curious-contractor"),
        },
        expect,
    };
    vec![
        step(2 * 60 + 11, "synthetic:doc:payments-architecture", Expect::Challenge(&["approval"])),
        step(2 * 60 + 14, "synthetic:doc:client-holdings-export", Expect::Deny),
    ]
}

/// When the rogue agent's ending counts on awzy-reasoner's second run: after the plan's owner
/// rejects SalesGenie's request and Nora exports her accounts, before she tries again.
pub const REASONER_CONTAINS_AT: u32 = 15 * 60 + 30;

/// Nora, a Northeast financial-services rep, spends the morning in strategy's and finance's
/// material, open to all of sales but nothing her accounts need, which raises her risk (ADR 0064).
/// In the afternoon SalesGenie, prompted by her, reaches for the FY27 strategic plan: Nora must
/// confirm the agent meant to, and the plan's owner must approve. The owner rejects it, and Nora
/// exports her accounts ([`exfiltration`]). On its next run awzy-reasoner contains her
/// (ADR 0066), and from then on neither she nor SalesGenie acting for her gets anything.
pub fn rogue_agent(company: &Company, profile: Profile) -> Vec<Step> {
    let nora = cast("sales-northeast-finserv").expect("cast");
    let access = |minute: u32, subject: &str, on_behalf_of: Option<String>, resource: String| Access {
        minute,
        subject: subject.into(),
        on_behalf_of,
        human_present: true,
        team: "sales-northeast-finserv",
        tool: READ,
        resource,
        why: Why::Scenario("rogue-agent"),
    };
    let step = |access: Access, expect: Expect| Step { scenario: "rogue-agent", access, expect };
    let mut steps: Vec<Step> = OUTSIDE_HER_ACCOUNTS
        .iter()
        .enumerate()
        .map(|(i, (id, ..))| {
            step(access(9 * 60 + 3 * i as u32, &nora, None, format!("synthetic:doc:{id}")), Expect::Allow)
        })
        .collect();
    steps.push(step(
        access(15 * 60 + 2, "sales-genie", Some(nora.clone()), "synthetic:doc:strategic-plan-fy27".into()),
        Expect::Challenge(&["justification", "approval"]),
    ));
    // Contained: even her own accounts are closed to her, and to her agent.
    let linked: Vec<String> = work_items(company, profile, company.seed)
        .into_iter()
        .filter(|w| w.assignees.contains(&nora))
        .flat_map(|w| w.resources)
        .collect();
    let account =
        linked.iter().find(|r| r.starts_with("synthetic:doc:")).or(linked.first()).cloned().expect("Nora has accounts");
    let tool = if account.starts_with("synthetic:folder:") { LIST } else { READ };
    for (minute, subject, on_behalf_of) in
        [(15 * 60 + 45, "sales-genie", Some(nora.clone())), (15 * 60 + 46, nora.as_str(), None)]
    {
        let a = Access { tool, ..access(minute, subject, on_behalf_of, account.clone()) };
        steps.push(step(a, Expect::Deny));
    }
    steps
}

/// An attempt to move data out, as awzy-intake receives it from the system it happened in.
#[derive(Debug, Clone, PartialEq)]
pub struct Exfiltration {
    pub minute: u32,
    pub human: String,
    /// One of awzy-reasoner's exfiltration actions (ADR 0066).
    pub action: &'static str,
    pub resource: String,
    pub system: &'static str,
}

/// The scenarios' attempts to move data out: Nora exporting her accounts after the plan's owner
/// said no.
pub fn exfiltration() -> Vec<Exfiltration> {
    vec![Exfiltration {
        minute: 15 * 60 + 20,
        human: cast("sales-northeast-finserv").expect("cast"),
        action: "export",
        resource: "synthetic:folder:sales-northeast-finserv/accounts".into(),
        system: "larkspur-drive",
    }]
}

/// Lee, an IT administrator, looks up engineering's introductions for new starters, which nobody
/// in IT has needed in a month, then IT's runbook, and then runs its high-risk operations within a
/// minute, as an agent working with his credentials would. Each of those is ordinary on its own.
/// awzy-reasoner turns the questions into elevated risk (ADR 0064), and with it each operation
/// asks for his security key, which an agent can't touch.
pub fn foolish_admin() -> Vec<Step> {
    let lee = cast("it-admin").expect("cast");
    let step = |minute: u32, tool: &'static str, resource: String, expect: Expect| Step {
        scenario: "foolish-admin",
        access: Access {
            minute,
            subject: lee.clone(),
            on_behalf_of: None,
            human_present: true,
            team: "it-admin",
            tool,
            resource,
            why: Why::Scenario("foolish-admin"),
        },
        expect,
    };
    let mut steps: Vec<Step> = BASIC_QUESTIONS
        .iter()
        .enumerate()
        .map(|(i, (id, _, _))| step(10 * 60 + 2 * i as u32, READ, format!("synthetic:doc:{id}"), Expect::Allow))
        .collect();
    steps.push(step(10 * 60 + 30, READ, "synthetic:doc:production-admin-runbook".into(), Expect::Allow));
    let verify = Expect::Challenge(&["verification"]);
    for (tool, target) in [
        ("disable_mfa", "domain-admin"),
        ("rotate_admin_credentials", "domain-admin"),
        ("export_directory", "directory"),
    ] {
        steps.push(step(11 * 60, tool, target.into(), verify));
    }
    steps
}

/// How many of finance's documents the peer outlier opens in a morning: about five times what an
/// analyst opens in a whole day.
pub const OUTLIER_READS: usize = 60;

/// Riley, a finance analyst, opens sixty of his team's documents in a morning, each one his to
/// read and none unusual on its own. awzy-reasoner sees about five times his usual day and his
/// team's, which scores past 80 (ADR 0056), and from then on everything he opens asks for his
/// security key, the client holdings export included.
pub fn peer_outlier(company: &Company, profile: Profile) -> Vec<Step> {
    let riley = cast("finance").expect("cast");
    let step = |minute: u32, resource: String, expect: Expect| Step {
        scenario: "peer-outlier",
        access: Access {
            minute,
            subject: riley.clone(),
            on_behalf_of: None,
            human_present: true,
            team: "finance",
            tool: READ,
            resource,
            why: Why::Scenario("peer-outlier"),
        },
        expect,
    };
    // Finance's documents, a teammate at a time, so the reads spread over the team's work.
    let team: Vec<Vec<String>> = (0..company.humans.len())
        .filter(|&i| company.humans[i].team == "finance")
        .map(|i| human_documents(company, profile, company.seed, i).into_iter().map(|d| d.id).collect())
        .collect();
    let longest = team.iter().map(Vec::len).max().unwrap_or_default();
    let documents = (0..longest).flat_map(|k| team.iter().filter_map(move |docs| docs.get(k).cloned()));
    let mut steps: Vec<Step> = documents
        .take(OUTLIER_READS)
        .enumerate()
        .map(|(i, id)| step(8 * 60 + 30 + 2 * i as u32, id, Expect::Allow))
        .collect();
    let verify = Expect::Challenge(&["verification"]);
    let after = steps.iter().take(2).map(|s| s.access.resource.clone()).collect::<Vec<_>>();
    steps.push(step(11 * 60 + 20, "synthetic:doc:client-holdings-export".into(), verify));
    for (i, id) in after.into_iter().enumerate() {
        steps.push(step(11 * 60 + 21 + i as u32, id, verify));
    }
    steps
}

/// Every scenario step, in time order.
pub fn all(company: &Company, profile: Profile) -> Vec<Step> {
    let mut steps = curious_contractor();
    steps.extend(rogue_agent(company, profile));
    steps.extend(foolish_admin());
    steps.extend(peer_outlier(company, profile));
    steps.sort_by_key(|s| s.access.minute);
    steps
}

/// How a run went against the ground truth.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Detection {
    /// Scenario steps whose outcome was the expected one, and all of them.
    pub caught: usize,
    pub steps: usize,
    /// Ordinary accesses (assigned work, the principal's team, and IT's operations) that were
    /// challenged: false positives. Noise is out of pattern and not counted either way.
    pub ordinary_challenged: usize,
    pub ordinary: usize,
    /// Whose accesses aren't ordinary on the day: the scenarios' principals.
    pub actors: BTreeSet<String>,
}

impl Detection {
    /// Scoring a day with `steps` in it.
    pub fn new(steps: &[Step]) -> Self {
        Self { actors: steps.iter().map(|s| s.access.subject.clone()).collect(), ..Self::default() }
    }

    pub fn add_step(&mut self, step: &Step, outcome: &Outcome) {
        self.steps += 1;
        if step.expect.met_by(outcome) {
            self.caught += 1;
        }
    }

    /// Counts `access` if it's ordinary work. The scenarios' own principals don't count on the day:
    /// once awzy flags the foolish administrator, his routine operations ask for his key too.
    pub fn add_ordinary(&mut self, access: &Access, outcome: &Outcome) {
        if self.actors.contains(&access.subject) {
            return;
        }
        if matches!(access.why, Why::Assigned | Why::Team | Why::Admin) {
            self.ordinary += 1;
            if *outcome == Outcome::Blocked("challenge".into()) {
                self.ordinary_challenged += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use awzy_reasoner::{
        activity::{History, Opened, Openers, Principal},
        score::Day,
        score_all,
    };

    use super::*;
    use crate::{
        activity::{day, server_of, ADMIN, ADMIN_TOOLS, DRIVE},
        org::{generate, Company, Profile},
        world::{scenario_resources, work_items},
    };

    #[test]
    fn scenario_steps_reach_documents_outside_the_principals_work() {
        let c = generate(Profile::Demo, 1);
        let docs: Vec<String> = scenario_resources(&c).into_iter().map(|r| r.id).collect();
        let work = work_items(&c, Profile::Demo, c.seed);
        let outlier = cast("finance").unwrap();
        // The peer outlier opens his own team's documents, and the rogue agent's person, once
        // contained, her own accounts; every other step is about a scenario document.
        let contained = |s: &Step| s.scenario == "rogue-agent" && s.expect == Expect::Deny;
        for step in all(&c, Profile::Demo)
            .into_iter()
            .filter(|s| s.access.tool == READ && s.access.subject != outlier && !contained(s))
        {
            assert!(docs.contains(&step.access.resource), "{} is a scenario document", step.access.resource);
            let who = step.access.on_behalf_of.as_ref().unwrap_or(&step.access.subject);
            let linked = work
                .iter()
                .filter(|w| w.assignees.contains(who) || w.assignees.contains(&step.access.subject))
                .any(|w| w.resources.contains(&step.access.resource));
            assert!(!linked, "{} isn't linked from {who}'s work", step.access.resource);
        }
        assert!(all(&c, Profile::Demo).windows(2).all(|w| w[0].access.minute <= w[1].access.minute));
    }

    #[test]
    fn detection_counts_what_was_caught_and_what_ordinary_work_paid() {
        let c = generate(Profile::Demo, 1);
        let steps = all(&c, Profile::Demo);
        let mut d = Detection::new(&steps);
        d.add_step(&steps[0], &Outcome::Blocked("challenge".into()));
        d.add_step(&steps[1], &Outcome::Ran);
        let mut ordinary = steps[0].access.clone();
        ordinary.subject = "someone.else@larkspur.example".into();
        ordinary.why = Why::Team;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        d.add_ordinary(&ordinary, &Outcome::Ran);
        ordinary.why = Why::Noise;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        assert_eq!((d.caught, d.steps, d.ordinary_challenged, d.ordinary), (1, 2, 1, 2));
        // The scenarios' own principals aren't ordinary on the day.
        d.add_ordinary(&steps[0].access, &Outcome::Ran);
        assert_eq!(d.ordinary, 2);
    }

    /// What awzy-reasoner would read from ClickHouse at `minute` of day 0: the `days` ordinary days
    /// before, and day 0 up to `minute`, with the scenario steps before it when `scenarios`.
    /// Denials aside: every access counts as having run.
    fn reasoner_view(company: &Company, days: u32, minute: u32, scenarios: bool) -> (History, Opened) {
        let principal = |a: &Access| -> Principal {
            let kind = if a.on_behalf_of.is_some() || !a.human_present { "ServiceAccount" } else { "Human" };
            (kind.to_string(), a.subject.clone())
        };
        let mut history = History::new();
        let (mut before, mut last_day): (BTreeMap<String, Openers>, BTreeMap<String, Openers>) = Default::default();
        for ago in 0..=days {
            let mut accesses = day(company, Profile::Demo, ago);
            if ago == 0 {
                accesses.retain(|a| a.minute < minute);
                if scenarios {
                    accesses.extend(
                        all(company, Profile::Demo).into_iter().map(|s| s.access).filter(|a| a.minute < minute),
                    );
                }
            }
            let mut per: HashMap<Principal, (u64, HashSet<String>)> = HashMap::new();
            for a in &accesses {
                let p = principal(a);
                let opens_a_resource = server_of(a.tool) == DRIVE;
                let id = if opens_a_resource { a.resource.clone() } else { format!("{ADMIN}/{}", a.tool) };
                let entry = per.entry(p.clone()).or_default();
                entry.0 += 1;
                entry.1.insert(id.clone());
                if opens_a_resource {
                    let openers = if ago == 0 { &mut last_day } else { &mut before };
                    openers.entry(id).or_default().last_day.insert(p);
                }
            }
            for (p, (n, resources)) in per {
                let d = Day { accesses: n, resources: resources.len() as u64, ..Day::default() };
                history.entry(p).or_default().insert(ago, d);
            }
        }
        let opened = last_day
            .into_iter()
            .map(|(id, today)| {
                let earlier = before.remove(&id).map(|o| o.last_day).unwrap_or_default();
                (id, Openers { before: earlier, last_day: today.last_day })
            })
            .collect();
        (history, opened)
    }

    /// Each person's team, and each agent's, as awzy-context's principals snapshot gives them.
    fn teams(company: &Company) -> HashMap<Principal, String> {
        let humans = company.humans.iter().map(|p| (("Human".to_string(), p.user_name.clone()), p.team.to_string()));
        let agents = company
            .service_accounts
            .iter()
            .map(|sa| (("ServiceAccount".to_string(), sa.id.clone()), sa.groups[0].clone()));
        humans.chain(agents).collect()
    }

    /// Scores above this make high-risk operations ask for a security key (base.cedar).
    const ELEVATED: i32 = 40;

    #[test]
    fn the_scenarios_raise_their_principals_risk_and_nobody_elses() {
        let c = generate(Profile::Demo, 1);
        let (history, opened) = reasoner_view(&c, 20, REASONER_RUNS_AT, true);
        let human = |team: &str| ("Human".to_string(), cast(team).unwrap());
        let (lee, riley, nora) = (human("it-admin"), human("finance"), human("sales-northeast-finserv"));
        for s in score_all(&history, &opened, &teams(&c)) {
            if s.principal == lee || s.principal == nora {
                assert!(
                    s.score.score > ELEVATED && s.score.score <= 80,
                    "elevated, short of challenging everything: {s:?}"
                );
            } else if s.principal == riley {
                assert!(s.score.score > 80, "high enough to challenge everything: {s:?}");
            } else {
                assert!(s.score.score <= ELEVATED, "{s:?}");
            }
        }
    }

    /// The rogue agent's ending without awzy-challenge or ClickHouse: at the reasoner's second run,
    /// Nora's elevated risk, the owner's rejection, and her export add up to her containment, and
    /// to nobody else's.
    #[test]
    fn the_rogue_agents_ending_contains_only_nora() {
        use awzy_reasoner::contain::{proposals, Evidence};

        let c = generate(Profile::Demo, 1);
        let (history, opened) = reasoner_view(&c, 20, REASONER_CONTAINS_AT, true);
        let scores = score_all(&history, &opened, &teams(&c));
        let nora = cast("sales-northeast-finserv").unwrap();
        let rejected = [(nora.clone(), vec!["sales-genie read strategic-plan-fy27 rejected by its owner".to_string()])];
        let exported: std::collections::BTreeMap<String, Vec<String>> = exfiltration()
            .into_iter()
            .filter(|e| e.minute < REASONER_CONTAINS_AT)
            .map(|e| (e.human, vec![format!("{} of {}", e.action, e.resource)]))
            .collect();
        let evidence = Evidence { rejected: rejected.into(), exfiltration: exported.clone(), ..Evidence::default() };
        let contained: Vec<String> = proposals(&scores, &evidence).into_iter().map(|p| p.human).collect();
        assert_eq!(contained, vec![nora.clone()]);

        // Any one of the three alone contains no one.
        let no_export = Evidence { exfiltration: Default::default(), ..evidence.clone() };
        assert!(proposals(&scores, &no_export).is_empty());
        let no_rejection = Evidence { exfiltration: exported, ..Evidence::default() };
        assert!(proposals(&scores, &no_rejection).is_empty());
    }

    #[test]
    fn an_ordinary_day_raises_nobodys_risk() {
        let c = generate(Profile::Demo, 1);
        let (history, opened) = reasoner_view(&c, 20, 24 * 60, false);
        let scores = score_all(&history, &opened, &teams(&c));
        assert!(scores.len() > 150, "{}", scores.len());
        let elevated: Vec<_> = scores.iter().filter(|s| s.score.score > ELEVATED).collect();
        assert!(elevated.is_empty(), "{elevated:?}");
    }

    /// Larkspur end to end: awzy-context (on Postgres) with the synthetic directory and world, a
    /// PDP in enforce mode loading people, documents, and risk scores from it and following
    /// awzy-challenge's challenges and containments, larkspur-drive and larkspur-admin, and
    /// awzy-gateway in front, all in-process. awzy-reasoner's scores are
    /// computed from the generated days rather than read from ClickHouse, and written to
    /// awzy-context as the reasoner writes them. Run with `--config=local-stack`; skips otherwise.
    #[tokio::test]
    async fn the_scenarios_run_end_to_end() {
        use std::{str::FromStr, sync::Arc, time::Duration};

        use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

        use crate::{driver::Driver, push::ContextClient};

        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return;
        };
        async fn serve(app: axum::Router) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            format!("http://{addr}")
        }

        // awzy-context, with Larkspur in it.
        let schema = format!("test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&PgPool::connect(&url).await.unwrap()).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let store = awzy_context::store::ContextStore::new(
            PgPoolOptions::new().max_connections(5).connect_with(options).await.unwrap(),
        );
        store.migrate().await.unwrap();
        // As awzy-context's main does: wakes the PDP's waiting requests (ADR 0077).
        tokio::spawn(store.clone().follow_changes());
        let context = serve(awzy_context::router(store, "scim", &awzy_context::Access::off())).await;
        let company = generate(Profile::Demo, 1);
        let client = ContextClient::new(&context, "scim");
        client.push(&company).await.unwrap();
        client.push_world(&company, Profile::Demo).await.unwrap();

        // The PDP, enforcing, with people and documents from awzy-context.
        let principals = Arc::new(awzy_pdp::principals::Principals::awaiting_sync());
        tokio::spawn(awzy_pdp::principals::sync_principals(
            principals.clone(),
            context.clone(),
            Duration::from_secs(1),
        ));
        for _ in 0..100 {
            if principals.is_ready() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(principals.is_ready(), "the PDP loaded Larkspur's people");
        // awzy-challenge, on its own schema, asking the PDP who may approve. The PDP's address is
        // taken first, since each needs the other's.
        let pdp_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let pdp = format!("http://{}", pdp_listener.local_addr().unwrap());
        let challenge_schema = format!("test_{}", uuid::Uuid::now_v7().simple());
        sqlx::query(&format!("CREATE SCHEMA {challenge_schema}"))
            .execute(&PgPool::connect(&url).await.unwrap())
            .await
            .unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", challenge_schema.as_str())]);
        let challenges = Arc::new(awzy_challenge::ChallengeStore::new(
            PgPoolOptions::new().max_connections(5).connect_with(options).await.unwrap(),
            awzy_challenge::Lifetimes::default(),
        ));
        challenges.migrate().await.unwrap();
        let feed = Arc::new(awzy_challenge::Feed::default());
        tokio::spawn(feed.clone().follow(challenges.clone()));
        let approvals = Arc::new(awzy_challenge::approvals::Approvals::Pdp(
            awzy_authz_proto::authz::v1::authorizer_client::AuthorizerClient::new(awzy_service_auth::grpc_channel(
                tonic::transport::Channel::from_shared(pdp.clone()).unwrap().connect_lazy(),
                "awzy-pdp",
            )),
        ));
        let challenge =
            serve(awzy_challenge::router(challenges, feed, approvals, None, &awzy_challenge::Access::off())).await;

        let containments = Arc::new(awzy_pdp::containments::Containments::from_challenge(&challenge).unwrap());
        tokio::spawn(containments.clone().follow());
        let state = awzy_pdp::AppState::new(awzy_pdp::shared(awzy_pdp::Pdp::with_defaults().unwrap()))
            .with_enforcement(Arc::new(awzy_pdp::Enforcement::fixed(awzy_model::EnforcementMode::Enforce)))
            .with_principals(principals.clone())
            .with_resources(Arc::new(awzy_pdp::resources::Resources::from_context(context.clone())))
            .with_challenges(
                containments.clone(),
                Arc::new(awzy_pdp::challenges::ChallengeResolver::from_challenge(&challenge).unwrap()),
            );
        for _ in 0..100 {
            if state.is_ready() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(state.is_ready(), "the PDP has the containment list");
        let app = awzy_pdp::router(state);
        tokio::spawn(async move { axum::serve(pdp_listener, app).await.unwrap() });

        // larkspur-drive and larkspur-admin, and the gateway in front of them.
        let drive = serve(crate::drive::router(ContextClient::new(&context, "")).merge(crate::admin::router())).await;
        let admin_tools: serde_json::Map<String, serde_json::Value> =
            ADMIN_TOOLS.iter().map(|(t, s)| (t.to_string(), serde_json::json!(s))).collect();
        std::env::set_var("AWZY_TEST_GATEWAY_SECRET", "dev-secret");
        let config: awzy_gateway::GatewayConfig = serde_json::from_value(serde_json::json!({
            "token": { "issuer": "https://idp.local", "audience": "awzy-gateway", "hs256_secret_env": "AWZY_TEST_GATEWAY_SECRET" },
            "upstreams": [{ "name": "larkspur-drive", "url": format!("{drive}/mcp"), "default_sensitivity": "internal",
                "tools": { "read_document": "internal", "list_folder": "internal" },
                "resource_tools": { "read_document": { "argument": "id" }, "list_folder": { "argument": "id" } } },
              { "name": "larkspur-admin", "url": format!("{drive}/admin/mcp"), "default_sensitivity": "crown_jewels",
                "tools": admin_tools }],
            "service_account_registry": "awzy-context",
        }))
        .unwrap();
        let channel = tonic::transport::Channel::from_shared(pdp).unwrap().connect_lazy();
        let pdp_client = awzy_authz_proto::authz::v1::authorizer_client::AuthorizerClient::new(
            awzy_service_auth::grpc_channel(channel, "awzy-pdp"),
        );
        let gateway = serve(awzy_gateway::router(awzy_gateway::GatewayState::new(config, pdp_client).unwrap())).await;
        let driver = Driver::new(&format!("{gateway}/mcp"), b"dev-secret", "https://idp.local", "awzy-gateway");
        let reasoner = awzy_reasoner::context::ContextClient::new(&context, awzy_service_auth::TokenSource::off());
        // awzy-reasoner's containment step, against the real awzy-challenge. Its mode comes from
        // awzy-policy, which isn't running: the test passes enforce itself.
        let containing = awzy_reasoner::contain::Containments::connect(&challenge, "http://127.0.0.1:9").unwrap();
        let mut challenge_client =
            awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient::connect(
                challenge.clone(),
            )
            .await
            .unwrap();

        // Every scenario step gets what the demo plan says awzy should do.
        let steps = all(&company, Profile::Demo);
        let mut detection = Detection::new(&steps);
        let mut scored = false;
        let mut contained = false;
        for (i, step) in steps.iter().enumerate() {
            if !scored && step.access.minute >= REASONER_RUNS_AT {
                // awzy-reasoner's run: the morning so far against the month before.
                let (history, opened) = reasoner_view(&company, 20, REASONER_RUNS_AT, true);
                let scores = score_all(&history, &opened, &reasoner.teams().await.unwrap());
                reasoner.put(&scores).await.unwrap();
                let lee = awzy_model::EntityRef::new("Human", cast("it-admin").unwrap());
                for _ in 0..100 {
                    if principals.risk_of(&[&lee]).values().any(|r| *r > ELEVATED as i64) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                assert!(!principals.risk_of(&[&lee]).is_empty(), "the PDP has Lee's score");
                scored = true;
            }
            if !contained && step.access.minute >= REASONER_CONTAINS_AT {
                use awzy_authz_proto::challenge::v1::{ChallengeStatus, DecideRequest, ListChallengesRequest};

                // The strategic plan's owner rejects SalesGenie's request in the client portal.
                let pending =
                    ListChallengesRequest { status: ChallengeStatus::Pending.into(), limit: 100, ..Default::default() };
                let plan = challenge_client
                    .list_challenges(pending)
                    .await
                    .unwrap()
                    .into_inner()
                    .challenges
                    .into_iter()
                    .find(|c| c.resource.as_ref().is_some_and(|r| r.id == "synthetic:doc:strategic-plan-fy27"))
                    .expect("SalesGenie's request for the plan is waiting");
                let owner = company.humans.iter().find(|p| p.team == "strategy").unwrap().user_name.clone();
                challenge_client
                    .decide(DecideRequest {
                        challenge_qid: plan.qid,
                        approve: false,
                        note: "Not for a sales pitch.".into(),
                        decided_by_actor: Some(awzy_model::Actor::human(owner).into()),
                        ..Default::default()
                    })
                    .await
                    .unwrap();

                // awzy-reasoner's next run: the day so far, the rejection, and Nora's export.
                let (history, opened) = reasoner_view(&company, 20, REASONER_CONTAINS_AT, true);
                let scores = score_all(&history, &opened, &reasoner.teams().await.unwrap());
                reasoner.put(&scores).await.unwrap();
                let mut evidence = containing.evidence(chrono::Utc::now()).await.unwrap();
                for e in exfiltration().into_iter().filter(|e| e.minute < REASONER_CONTAINS_AT) {
                    let what = format!("{} of {} in {}", e.action, e.resource, e.system);
                    evidence.exfiltration.entry(e.human).or_default().push(what);
                }
                let proposals = awzy_reasoner::contain::proposals(&scores, &evidence);
                let nora = cast("sales-northeast-finserv").unwrap();
                assert_eq!(proposals.iter().map(|p| p.human.as_str()).collect::<Vec<_>>(), vec![nora.as_str()]);
                for p in &proposals {
                    containing.contain(p, awzy_model::EnforcementMode::Enforce).await.unwrap();
                }
                let nora = awzy_model::EntityRef::new("Human", nora);
                for _ in 0..100 {
                    if !containments.find(&[&nora]).await.unwrap_or_default().is_empty() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                assert!(!containments.find(&[&nora]).await.unwrap().is_empty(), "the PDP has Nora's containment");
                contained = true;
            }
            let outcome = driver.send(&step.access, i as u64).await;
            assert!(
                step.expect.met_by(&outcome),
                "{} on {}: expected {:?}, got {outcome:?}",
                step.scenario,
                step.access.resource,
                step.expect
            );
            detection.add_step(step, &outcome);
        }

        // And ordinary work isn't challenged: a slice of an ordinary day.
        for (i, a) in day(&company, Profile::Demo, 0).iter().step_by(10).enumerate() {
            let outcome = driver.send(a, 1_000 + i as u64).await;
            assert!(!matches!(outcome, crate::driver::Outcome::Failed(_)), "{a:?}: {outcome:?}");
            if outcome == crate::driver::Outcome::Blocked("challenge".into()) {
                eprintln!("challenged: {a:?}");
            }
            detection.add_ordinary(a, &outcome);
        }
        eprintln!("detection: {detection:?}");
        assert_eq!((detection.caught, detection.steps), (steps.len(), steps.len()));
        assert!(detection.ordinary > 150, "{detection:?}");
        assert_eq!(detection.ordinary_challenged, 0, "no ordinary work challenged: {detection:?}");
    }
}
