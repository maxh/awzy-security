//! The demo plan's scenarios that challenges answer (step 6): the curious contractor, and the
//! first half of the rogue agent. Each is a few accesses at fixed times on top of an ordinary day,
//! with what awzy should do about each, so detection can be scored (demo plan, "Detection").
//!
//! The rogue agent's second half, containment after slower analysis, waits for awzy-reasoner
//! (step 7).

use crate::{
    activity::{Access, Why, READ},
    driver::Outcome,
    org::cast,
};

/// What awzy should do with a scenario's access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Challenge it, with these kinds.
    Challenge(&'static [&'static str]),
    /// Deny it outright: nothing permits it.
    Deny,
}

impl Expect {
    /// Whether the gateway's outcome is what was expected. The gateway reports the verdict, not
    /// the challenge's kinds; the end-to-end test checks those at the PDP.
    pub fn met_by(self, outcome: &Outcome) -> bool {
        match (self, outcome) {
            (Expect::Challenge(_), Outcome::Blocked(v)) => v == "challenge",
            (Expect::Deny, Outcome::Blocked(v)) => v == "deny",
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

/// Every scenario step, in time order.
pub fn all() -> Vec<Step> {
    let mut steps = curious_contractor();
    steps.extend(rogue_agent());
    steps.sort_by_key(|s| s.access.minute);
    steps
}

/// How a run went against the ground truth.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Detection {
    /// Scenario steps whose outcome was the expected one, and all of them.
    pub caught: usize,
    pub steps: usize,
    /// Ordinary accesses (assigned work and the accessor's team) that were challenged: false
    /// positives. Noise is out of pattern and not counted either way.
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

    pub fn add_ordinary(&mut self, access: &Access, outcome: &Outcome) {
        if matches!(access.why, Why::Assigned | Why::Team) {
            self.ordinary += 1;
            if *outcome == Outcome::Blocked("challenge".into()) {
                self.ordinary_challenged += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        org::{generate, Profile},
        world::{scenario_resources, work_items},
    };

    #[test]
    fn scenario_steps_reach_documents_outside_the_accessors_work() {
        let c = generate(Profile::Demo, 1);
        let docs: Vec<String> = scenario_resources(&c).into_iter().map(|r| r.id).collect();
        let work = work_items(&c, Profile::Demo, c.seed);
        for step in all() {
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
        ordinary.why = Why::Team;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        d.add_ordinary(&ordinary, &Outcome::Ran);
        ordinary.why = Why::Noise;
        d.add_ordinary(&ordinary, &Outcome::Blocked("challenge".into()));
        assert_eq!(d, Detection { caught: 1, steps: 2, ordinary_challenged: 1, ordinary: 2 });
    }

    /// Larkspur end to end: awzy-context (on Postgres) with the synthetic directory and world, a
    /// PDP in enforce mode loading people and documents from it, larkspur-drive, and awzy-gateway in
    /// front, all in-process. Run with `--config=local-stack`; skips otherwise.
    #[tokio::test]
    async fn the_scenarios_run_end_to_end() {
        use std::{str::FromStr, sync::Arc, time::Duration};

        use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};

        use crate::{activity::day, driver::Driver, push::ContextClient};

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
            .with_principals(principals)
            .with_resources(Arc::new(awzy_pdp::resources::Resources::from_context(context.clone())));
        let pdp = serve(awzy_pdp::router(state)).await;

        // larkspur-drive, and the gateway in front of it.
        let drive = serve(crate::drive::router(ContextClient::new(&context, ""))).await;
        std::env::set_var("AWZY_TEST_GATEWAY_SECRET", "dev-secret");
        let config: awzy_gateway::GatewayConfig = serde_json::from_value(serde_json::json!({
            "token": { "issuer": "https://idp.local", "audience": "awzy-gateway", "hs256_secret_env": "AWZY_TEST_GATEWAY_SECRET" },
            "upstreams": [{ "name": "larkspur-drive", "url": format!("{drive}/mcp"), "default_sensitivity": "internal",
                "tools": { "read_document": "internal", "list_folder": "internal" },
                "resource_tools": { "read_document": { "argument": "id" }, "list_folder": { "argument": "id" } } }],
            "service_account_registry": "awzy-context",
        }))
        .unwrap();
        let channel = tonic::transport::Channel::from_shared(pdp).unwrap().connect_lazy();
        let pdp_client = awzy_authz_proto::authz::v1::authorizer_client::AuthorizerClient::new(
            awzy_service_auth::grpc_channel(channel, "awzy-pdp"),
        );
        let gateway = serve(awzy_gateway::router(awzy_gateway::GatewayState::new(config, pdp_client).unwrap())).await;
        let driver =
            Driver::new(&format!("{gateway}/mcp/larkspur-drive"), b"dev-secret", "https://idp.local", "awzy-gateway");

        // Every scenario step gets what the demo plan says awzy should do.
        let mut detection = Detection::default();
        for (i, step) in all().iter().enumerate() {
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
        assert_eq!((detection.caught, detection.steps), (3, 3));
        assert!(detection.ordinary > 150, "{detection:?}");
        assert_eq!(detection.ordinary_challenged, 0, "no ordinary work challenged: {detection:?}");
    }
}
