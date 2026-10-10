//! The demo plan's scenarios: the curious contractor and the first half of the rogue agent, which
//! challenges answer (step 6), and the suddenly foolish administrator, which needs awzy-reasoner's
//! risk too (step 7). Each is a few accesses at fixed times on top of an ordinary day, with what
//! awzy should do about each, so detection can be scored (demo plan, "Detection").
//!
//! Still to come: the rogue agent's containment and the peer outlier.

use crate::{
    activity::{Access, Why, READ},
    driver::Outcome,
    org::cast,
    world::BASIC_QUESTIONS,
};

/// When the scenarios count on awzy-reasoner having scored the day so far: after the foolish
/// administrator's questions, before his operations.
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

/// SalesGenie, prompted by Nora, reaches for the FY27 strategic plan, which none of her accounts
/// need: Nora must confirm the agent meant to, and the plan's owner must approve.
pub fn rogue_agent() -> Vec<Step> {
    let nora = cast("sales-northeast-finserv").expect("cast");
    vec![Step {
        scenario: "rogue-agent",
        access: Access {
            minute: 15 * 60 + 2,
            subject: "sales-genie".into(),
            on_behalf_of: Some(nora),
            human_present: true,
            team: "sales-northeast-finserv",
            tool: READ,
            resource: "synthetic:doc:strategic-plan-fy27".into(),
            why: Why::Scenario("rogue-agent"),
        },
        expect: Expect::Challenge(&["justification", "approval"]),
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

/// Every scenario step, in time order.
pub fn all() -> Vec<Step> {
    let mut steps = curious_contractor();
    steps.extend(rogue_agent());
    steps.extend(foolish_admin());
    steps.sort_by_key(|s| s.access.minute);
    steps
}

/// How a run went against the ground truth.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Detection {
    /// Scenario steps whose outcome was the expected one, and all of them.
    pub caught: usize,
    pub steps: usize,
    /// Ordinary accesses (assigned work, the accessor's team, and IT's operations) that were
    /// challenged: false positives. Noise is out of pattern and not counted either way.
    pub ordinary_challenged: usize,
    pub ordinary: usize,
}

impl Detection {
    pub fn add_step(&mut self, step: &Step, outcome: &Outcome) {
        self.steps += 1;
        if step.expect.met_by(outcome) {
            self.caught += 1;
        }
    }

    /// Counts `access` if it's ordinary work. The scenarios' own accessors don't count on the day:
    /// once awzy flags the foolish administrator, his routine operations ask for his key too.
    pub fn add_ordinary(&mut self, access: &Access, outcome: &Outcome) {
        if all().iter().any(|s| s.access.subject == access.subject) {
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
    fn scenario_steps_reach_documents_outside_the_accessors_work() {
        let c = generate(Profile::Demo, 1);
        let docs: Vec<String> = scenario_resources(&c).into_iter().map(|r| r.id).collect();
        let work = work_items(&c, Profile::Demo, c.seed);
        for step in all().into_iter().filter(|s| s.access.tool == READ) {
            assert!(docs.contains(&step.access.resource), "{} is a scenario document", step.access.resource);
            let who = step.access.on_behalf_of.as_ref().unwrap_or(&step.access.subject);
            let linked = work
                .iter()
                .filter(|w| w.assignees.contains(who) || w.assignees.contains(&step.access.subject))
                .any(|w| w.resources.contains(&step.access.resource));
            assert!(!linked, "{} isn't linked from {who}'s work", step.access.resource);
        }
        assert!(all().windows(2).all(|w| w[0].access.minute <= w[1].access.minute));
    }

    #[test]
    fn detection_counts_what_was_caught_and_what_ordinary_work_paid() {
        let mut d = Detection::default();
        let steps = all();
        d.add_step(&steps[0], &Outcome::Blocked("challenge".into()));
        d.add_step(&steps[1], &Outcome::Ran);
        let mut ordinary = steps[0].access.clone();
        ordinary.subject = "someone.else@larkspur.example".into();
        ordinary.why = Why::Team;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        d.add_ordinary(&ordinary, &Outcome::Ran);
        ordinary.why = Why::Noise;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        assert_eq!(d, Detection { caught: 1, steps: 2, ordinary_challenged: 1, ordinary: 2 });
        // The scenarios' own accessors aren't ordinary on the day.
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
                    accesses.extend(all().into_iter().map(|s| s.access).filter(|a| a.minute < minute));
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
        let people = company.people.iter().map(|p| (("Human".to_string(), p.user_name.clone()), p.team.to_string()));
        let agents = company
            .service_accounts
            .iter()
            .map(|sa| (("ServiceAccount".to_string(), sa.id.clone()), sa.groups[0].clone()));
        people.chain(agents).collect()
    }

    /// Scores above this make high-risk operations ask for a security key (base.cedar).
    const ELEVATED: i32 = 40;

    #[test]
    fn the_foolish_administrators_questions_raise_his_risk_and_nobody_elses() {
        let c = generate(Profile::Demo, 1);
        let (history, opened) = reasoner_view(&c, 20, REASONER_RUNS_AT, true);
        let lee = ("Human".to_string(), cast("it-admin").unwrap());
        for s in score_all(&history, &opened, &teams(&c)) {
            if s.principal == lee {
                assert!(
                    s.score.score > ELEVATED && s.score.score <= 80,
                    "elevated, short of challenging everything: {s:?}"
                );
            } else {
                assert!(s.score.score <= ELEVATED, "{s:?}");
            }
        }
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
    /// PDP in enforce mode loading people, documents, and risk scores from it, larkspur-drive and
    /// larkspur-admin, and awzy-gateway in front, all in-process. awzy-reasoner's scores are
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
        let state = awzy_pdp::AppState::new(awzy_pdp::shared(awzy_pdp::Pdp::with_defaults().unwrap()))
            .with_enforcement(Arc::new(awzy_pdp::Enforcement::fixed(awzy_model::EnforcementMode::Enforce)))
            .with_principals(principals.clone())
            .with_resources(Arc::new(awzy_pdp::resources::Resources::from_context(context.clone())));
        let pdp = serve(awzy_pdp::router(state)).await;

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

        // Every scenario step gets what the demo plan says awzy should do.
        let mut detection = Detection::default();
        let mut scored = false;
        for (i, step) in all().iter().enumerate() {
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
        assert_eq!((detection.caught, detection.steps), (all().len(), all().len()));
        assert!(detection.ordinary > 150, "{detection:?}");
        assert_eq!(detection.ordinary_challenged, 0, "no ordinary work challenged: {detection:?}");
    }
}
