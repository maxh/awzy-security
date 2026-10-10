//! awzy-synthetic directory|world|all [--profile demo|scale] [--seed N] [--print]
//! awzy-synthetic drive
//! awzy-synthetic activity [--profile demo|scale] [--seed N] [--days N] [--rate N]
//! awzy-synthetic history [--profile demo|scale] [--seed N] [--days N]
//!
//! `directory` sends the synthetic company's people, groups, and service accounts to awzy-context
//! at AWZY_CONTEXT_URL (default http://127.0.0.1:8084) with the SCIM token in AWZY_SCIM_TOKEN.
//! `world` sends its resources and work items, which name those people, so it runs after
//! `directory`; `all` runs both. `--print` writes the company as JSON instead and sends nothing.
//! `drive` serves larkspur-drive, an MCP server over the documents in awzy-context, at
//! AWZY_LISTEN_ADDR (default 127.0.0.1:8095) `/mcp`, and larkspur-admin, IT's high-risk
//! operations, at `/admin/mcp`, for awzy-gateway to front (ADR 0050, 0064).
//! `activity` makes the company's everyday tool calls on both through the gateway at
//! AWZY_GATEWAY_URL, `--rate` per second (default 5) for `--days` generated days (default 1),
//! with tokens signed by the gateway's development secret in AWZY_GATEWAY_DEV_SECRET. The first
//! day includes the demo plan's scenarios (`--no-scenarios` leaves them out), and the run reports
//! how many awzy caught and how much ordinary work it challenged. With AWZY_INTAKE_URL and
//! AWZY_INTAKE_TOKEN, the scenarios' attempts to move data out go to awzy-intake as they happen.
//! `history` sends the `--days` workdays before today (default 30) to awzy-intake at
//! AWZY_INTAKE_URL as app events, with its shippers' token in AWZY_INTAKE_TOKEN (demo plan 5c).

use anyhow::{bail, Context};
use awzy_synthetic::{
    org::{generate, Profile},
    push::ContextClient,
};

const USAGE: &str =
    "usage: awzy-synthetic directory|world|all [--profile demo|scale] [--seed N] [--print] | awzy-synthetic drive \
     | awzy-synthetic activity [--profile demo|scale] [--seed N] [--days N] [--rate N] [--no-scenarios] \
     | awzy-synthetic history [--profile demo|scale] [--seed N] [--days N]";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-synthetic");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let command = args.first().map(String::as_str).unwrap_or_default();
    let url = std::env::var("AWZY_CONTEXT_URL").unwrap_or_else(|_| "http://127.0.0.1:8084".into());
    if command == "drive" {
        let addr = std::env::var("AWZY_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:8095".into());
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!(%addr, context = %url, "larkspur-drive serving MCP at /mcp, larkspur-admin at /admin/mcp");
        let app = awzy_synthetic::drive::router(ContextClient::new(&url, "")).merge(awzy_synthetic::admin::router());
        axum::serve(listener, app).await?;
        return Ok(());
    }
    let (directory, world) = match command {
        "activity" => return activity(&args).await,
        "history" => return history(&args).await,
        "directory" => (true, false),
        "world" => (false, true),
        "all" => (true, true),
        _ => bail!(USAGE),
    };
    let profile_name = flag("--profile").unwrap_or_else(|| "demo".into());
    let profile = Profile::parse(&profile_name).with_context(|| format!("unknown profile {profile_name:?}"))?;
    let seed: u64 = flag("--seed").map(|s| s.parse()).transpose()?.unwrap_or(1);
    let company = generate(profile, seed);

    if args.iter().any(|a| a == "--print") {
        println!("{}", serde_json::to_string_pretty(&company)?);
        return Ok(());
    }
    // Only SCIM needs the token; the resource API is cluster-internal.
    let token = match std::env::var("AWZY_SCIM_TOKEN") {
        Ok(t) => t,
        Err(_) if !directory => String::new(),
        Err(_) => bail!("AWZY_SCIM_TOKEN is not set"),
    };
    let client = ContextClient::new(&url, &token);
    if directory {
        let summary = client.push(&company).await?;
        tracing::info!(?summary, profile = profile_name, seed, "synthetic directory provisioned");
    }
    if world {
        let summary = client.push_world(&company, profile).await?;
        tracing::info!(?summary, profile = profile_name, seed, "synthetic resources and work items written");
    }
    Ok(())
}

/// Replays generated days of activity through the gateway at a steady rate, and reports what
/// became of the calls. The decisions themselves are in the client portal's Decisions page.
async fn activity(args: &[String]) -> anyhow::Result<()> {
    use awzy_synthetic::{
        activity,
        driver::{Driver, Tally},
        scenarios::{self, Detection},
    };
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let profile_name = flag("--profile").unwrap_or_else(|| "demo".into());
    let profile = Profile::parse(&profile_name).with_context(|| format!("unknown profile {profile_name:?}"))?;
    let seed: u64 = flag("--seed").map(|s| s.parse()).transpose()?.unwrap_or(1);
    let days: u32 = flag("--days").map(|s| s.parse()).transpose()?.unwrap_or(1);
    let rate: f64 = flag("--rate").map(|s| s.parse()).transpose()?.unwrap_or(5.0);
    anyhow::ensure!(rate > 0.0, "--rate is calls per second, more than 0");

    let env = |k: &str, default: &str| std::env::var(k).unwrap_or_else(|_| default.to_string());
    let gateway = std::env::var("AWZY_GATEWAY_URL").context("AWZY_GATEWAY_URL is not set")?;
    let secret = std::env::var("AWZY_GATEWAY_DEV_SECRET").context("AWZY_GATEWAY_DEV_SECRET is not set")?;
    let endpoint = format!("{}/mcp", gateway.trim_end_matches('/'));
    let driver = Driver::new(
        &endpoint,
        secret.as_bytes(),
        &env("AWZY_TOKEN_ISSUER", "https://idp.local"),
        &env("AWZY_TOKEN_AUDIENCE", "awzy-gateway"),
    );

    let with_scenarios = !args.iter().any(|a| a == "--no-scenarios");

    let company = generate(profile, seed);
    let mut tally = Tally::default();
    let steps = scenarios::all(&company, profile);
    let intake = match (std::env::var("AWZY_INTAKE_URL"), std::env::var("AWZY_INTAKE_TOKEN")) {
        (Ok(url), Ok(token)) => Some(awzy_synthetic::history::Intake::new(&url, &token)),
        _ => None,
    };
    let mut exports = if with_scenarios { scenarios::exfiltration() } else { vec![] };
    let mut detection = Detection::new(if with_scenarios { &steps } else { &[] });
    let mut tick = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / rate));
    let mut id = 0;
    for d in 0..days {
        if d == 1 {
            // After the scenarios' day, their accessors' work is ordinary again.
            detection.actors.clear();
        }
        // The scenarios happen on the first day, among its ordinary accesses.
        let mut calls: Vec<(activity::Access, Option<scenarios::Step>)> =
            activity::day(&company, profile, d).into_iter().map(|a| (a, None)).collect();
        if d == 0 && with_scenarios {
            calls.extend(steps.iter().map(|s| (s.access.clone(), Some(s.clone()))));
            calls.sort_by_key(|(a, _)| a.minute);
        }
        tracing::info!(day = d, calls = calls.len(), %endpoint, "replaying a day of activity");
        for (a, step) in &calls {
            tick.tick().await;
            // The scenarios' exports, when their minute comes, if awzy-intake is there to hear them.
            while d == 0 && exports.first().is_some_and(|e| e.minute <= a.minute) {
                let e = exports.remove(0);
                match &intake {
                    Some(intake) => {
                        let event = awzy_synthetic::history::exfiltration_event(chrono::Utc::now(), &e);
                        if let Err(err) = intake.send(&[event]).await {
                            tracing::warn!(error = %err, human = %e.human, "export not sent to awzy-intake");
                        }
                    }
                    None => {
                        tracing::info!(human = %e.human, "AWZY_INTAKE_URL is not set; the scenario's export isn't sent")
                    }
                }
            }
            id += 1;
            let outcome = driver.send(a, id).await;
            if let awzy_synthetic::driver::Outcome::Failed(why) = &outcome {
                tracing::warn!(subject = %a.subject, resource = %a.resource, %why, "call failed");
            }
            match step {
                Some(step) => {
                    let caught = step.expect.met_by(&outcome);
                    tracing::info!(scenario = step.scenario, subject = %a.subject, resource = %a.resource,
                        expected = ?step.expect, ?outcome, caught, "scenario step");
                    detection.add_step(step, &outcome);
                }
                None => detection.add_ordinary(a, &outcome),
            }
            tally.add(&outcome);
            if id % 500 == 0 {
                tracing::info!(calls = id, ?tally, "activity so far");
            }
        }
    }
    tracing::info!(calls = id, ?tally, "activity done");
    if with_scenarios {
        // In monitor mode the gateway allows everything, so nothing is caught here; the would-be
        // verdicts are on the Decisions page.
        tracing::info!(
            caught = detection.caught,
            steps = detection.steps,
            ordinary_challenged = detection.ordinary_challenged,
            ordinary = detection.ordinary,
            "detection"
        );
    }
    Ok(())
}

/// Sends a backdated month of activity to awzy-intake, a day at a time (demo plan step 5c).
async fn history(args: &[String]) -> anyhow::Result<()> {
    use awzy_synthetic::history::{self, Intake};
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let profile_name = flag("--profile").unwrap_or_else(|| "demo".into());
    let profile = Profile::parse(&profile_name).with_context(|| format!("unknown profile {profile_name:?}"))?;
    let seed: u64 = flag("--seed").map(|s| s.parse()).transpose()?.unwrap_or(1);
    let days: usize = flag("--days").map(|s| s.parse()).transpose()?.unwrap_or(30);
    let url = std::env::var("AWZY_INTAKE_URL").context("AWZY_INTAKE_URL is not set")?;
    let token = std::env::var("AWZY_INTAKE_TOKEN").context("AWZY_INTAKE_TOKEN is not set")?;
    let intake = Intake::new(&url, &token);
    let company = generate(profile, seed);
    let today = chrono::Utc::now().date_naive();
    let mut total = 0;
    for (i, events) in history::days(&company, profile, today, days).enumerate() {
        total += intake.send(&events).await?;
        tracing::info!(day = i + 1, of = days, events = events.len(), total, "history sent");
    }
    tracing::info!(total, days, "history done");
    Ok(())
}
