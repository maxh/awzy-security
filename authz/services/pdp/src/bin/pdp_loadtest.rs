//! Load test for a running PDP: `bazel run //authz/services/pdp:pdp_loadtest -- http://127.0.0.1:8082 32 20`
//! (endpoint, concurrent clients, seconds). Sends a mix of checks over gRPC, a third of them
//! delegated (two Cedar evaluations), and prints throughput and latency percentiles. Each client
//! has its own connection.
//!
//! With a fourth argument, a rate in checks per second, it's open loop: checks are sent on a fixed
//! schedule however slowly the PDP answers, spread over the clients' connections, as independent
//! enforcement points send them. Without it, each client waits for its answer before the next
//! check, so a slow PDP is sent less, and the latencies understate the tail (coordinated
//! omission). Open loop measures each check from when it was due, and prints a line a minute, so
//! a run long enough to use up burstable nodes' CPU credits shows when latency changes.
//!
//! A fifth argument sets that line's window in seconds. A game day (#353) uses 1, to time what the
//! PDPs do while a dependency is away: each line has the time, the verdicts, and the errors by
//! gRPC code. In open loop a check unanswered after 2 seconds counts as an error, as the gateway
//! gives up on it then.
//!
//! Entities are sent inline, as enforcement points do until awzy-context exists. With
//! `AWZY_TEST_LOADTEST_RESOURCES` (comma-separated resource IDs awzy-context knows), checks name
//! those instead and send no entity for them, so the PDP decides on awzy-context's attributes and
//! stale limits, as for a real install's resources (#402).
//!
//! In a cluster it runs from the PDP's image and sends the pod's service token for `awzy-pdp`
//! (ADR 0054), mounted as awzy's workloads mount theirs; without one it sends none.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use std::collections::BTreeMap;

use awzy_authz_proto::{
    authz::v1::{authorizer_client::AuthorizerClient, CheckRequest, Entity, EntityRef, Verdict},
    json_to_struct,
};
use serde_json::json;

fn r(kind: &str, id: &str) -> EntityRef {
    EntityRef { r#type: kind.into(), id: id.into() }
}

fn entity(kind: &str, id: &str, attrs: serde_json::Value, groups: &[&str]) -> Entity {
    Entity {
        uid: Some(r(kind, id)),
        attrs: attrs.as_object().map(|o| json_to_struct(o.clone())),
        parents: groups.iter().map(|g| r("Group", g)).collect(),
    }
}

/// Resource IDs from `AWZY_TEST_LOADTEST_RESOURCES`, if set.
fn known_resources() -> &'static [String] {
    static KNOWN: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    KNOWN.get_or_init(|| {
        std::env::var("AWZY_TEST_LOADTEST_RESOURCES")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// Request `i` of the mix: humans and a delegated service account, across sensitivities.
fn request(i: usize) -> CheckRequest {
    let resources = [("design-doc", "internal"), ("q3-strategy", "confidential"), ("board-deck", "crown_jewels")];
    let mut entities = vec![
        entity("Human", "alice", json!({}), &["security"]),
        entity("Human", "bob", json!({}), &["engineering"]),
    ];
    let known = known_resources();
    let res_id = match known.is_empty() {
        true => {
            let (id, sensitivity) = resources[i % resources.len()];
            entities.push(entity("Resource", id, json!({ "sensitivity": sensitivity }), &[]));
            id
        }
        false => known[i % known.len()].as_str(),
    };
    let delegated = i.is_multiple_of(3);
    let (principal, on_behalf_of) = if delegated {
        entities.push(entity(
            "ServiceAccount",
            "sales-genie",
            json!({ "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true }),
            &["engineering"],
        ));
        (r("ServiceAccount", "sales-genie"), Some(r("Human", if i.is_multiple_of(2) { "alice" } else { "bob" })))
    } else {
        (r("Human", if i.is_multiple_of(2) { "alice" } else { "bob" }), None)
    };
    CheckRequest {
        principal: Some(principal),
        action: "read".into(),
        resource: Some(r("Resource", res_id)),
        context: Some(json_to_struct(serde_json::Map::from_iter([("client".to_string(), json!("loadtest"))]))),
        on_behalf_of,
        entities,
        human_present: Some(true),
        always_enforced: false,
        work_item_id: None,
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let endpoint = args.get(1).cloned().unwrap_or_else(|| "http://127.0.0.1:8082".into());
    let clients: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(16);
    let seconds: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(10);
    let rate: Option<f64> = args.get(4).map(|s| s.parse()).transpose()?;
    let window: u64 = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(60);

    // One connection per client, as separate enforcement points have: a Service spreads
    // connections across PDP pods, not requests, so one shared connection would load one pod.
    let mut connections = Vec::new();
    for _ in 0..clients {
        let channel = tonic::transport::Channel::from_shared(endpoint.clone())?.connect().await?;
        connections.push(AuthorizerClient::new(awzy_service_auth::grpc_channel(channel, "awzy-pdp")));
    }
    // Warm up the connections and the PDPs.
    for (i, client) in (0..200).zip(connections.iter().cycle()) {
        client.clone().check(request(i)).await?;
    }

    let (all, errors) = match rate {
        Some(rate) => open_loop(connections, rate, seconds, window).await?,
        None => closed_loop(connections, seconds).await?,
    };
    if all.is_empty() {
        anyhow::bail!("no successful checks ({errors} errors)");
    }
    match rate {
        Some(rate) => println!(
            "endpoint {endpoint}, open loop at {rate:.0}/s over {clients} connections, {seconds}s, 1 in 3 delegated (two Cedar checks); latency from when each check was due"
        ),
        None => println!("endpoint {endpoint}, {clients} clients, {seconds}s, 1 in 3 delegated (two Cedar checks)"),
    }
    println!("checks   {} ok, {errors} errors, {:.0}/s", all.len(), all.len() as f64 / seconds as f64);
    for (label, p) in [("p50", 0.50), ("p95", 0.95), ("p99", 0.99), ("p99.9", 0.999)] {
        println!("{label:8} {:.2} ms", percentile(&all, p).as_secs_f64() * 1000.0);
    }
    println!("max      {:.2} ms", all.last().copied().unwrap_or_default().as_secs_f64() * 1000.0);
    Ok(())
}

type Client = AuthorizerClient<awzy_service_auth::AuthChannel>;

/// Retried once like an enforcement point does, so a rollout counts what they'd see.
async fn check(client: &Client, i: usize) -> bool {
    ask(client, i).await.is_ok()
}

async fn ask(client: &Client, i: usize) -> Result<Verdict, tonic::Status> {
    let ask = || {
        let mut client = client.clone();
        async move { client.check(request(i)).await }
    };
    let response = awzy_service_auth::retry_once_if_unanswered(ask).await?;
    Ok(Verdict::try_from(response.get_ref().verdict).unwrap_or(Verdict::Unspecified))
}

/// How long an open-loop check waits, as the gateway waits for a PDP before its outage rules apply.
const OPEN_LOOP_TIMEOUT: Duration = Duration::from_secs(2);

/// What an open-loop check got: a verdict and its latency from when it was due, or an error's code.
type Outcome = Result<(Verdict, Duration), tonic::Code>;

/// Each client sends its next check when the last is answered. Sorted latencies, and errors.
async fn closed_loop(connections: Vec<Client>, seconds: u64) -> anyhow::Result<(Vec<Duration>, usize)> {
    let clients = connections.len();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut workers = Vec::new();
    for (w, client) in connections.into_iter().enumerate() {
        workers.push(tokio::spawn(async move {
            let mut latencies = Vec::new();
            let mut errors = 0usize;
            let mut i = w;
            while Instant::now() < deadline {
                let start = Instant::now();
                if check(&client, i).await {
                    latencies.push(start.elapsed());
                } else {
                    errors += 1;
                }
                i += clients;
            }
            (latencies, errors)
        }));
    }
    let mut all = Vec::new();
    let mut errors = 0;
    for w in workers {
        let (l, e) = w.await?;
        all.extend(l);
        errors += e;
    }
    all.sort();
    Ok((all, errors))
}

/// Checks are due every `1 / rate` seconds whatever the PDP does, and each is timed from when it
/// was due, so time spent queued behind a slow PDP counts. Sorted latencies, and errors.
async fn open_loop(
    connections: Vec<Client>,
    rate: f64,
    seconds: u64,
    window_secs: u64,
) -> anyhow::Result<(Vec<Duration>, usize)> {
    anyhow::ensure!(rate > 0.0, "the rate is in checks per second, above 0");
    anyhow::ensure!(window_secs > 0, "the window is in seconds, above 0");
    let (tx, mut rx) = mpsc::unbounded_channel::<Outcome>();
    let start = Instant::now();
    let total = (rate * seconds as f64) as usize;
    let sender = tokio::spawn(async move {
        for i in 0..total {
            let due = start + Duration::from_secs_f64(i as f64 / rate);
            // Timers tick in milliseconds: checks that fall due within one tick go out together,
            // each still timed from its own due time.
            tokio::time::sleep_until(due.into()).await;
            let client = connections[i % connections.len()].clone();
            let tx = tx.clone();
            tokio::spawn(async move {
                let outcome = match tokio::time::timeout(OPEN_LOOP_TIMEOUT, ask(&client, i)).await {
                    Ok(Ok(verdict)) => Ok((verdict, due.elapsed())),
                    Ok(Err(status)) => Err(status.code()),
                    Err(_) => Err(tonic::Code::DeadlineExceeded),
                };
                let _ = tx.send(outcome);
            });
        }
    });

    // A line a window (a minute by default): the checks answered in it, each timed from when it
    // was due.
    let every = Duration::from_secs(window_secs);
    let mut all = Vec::with_capacity(total);
    let mut window = Window::default();
    let mut errors = 0usize;
    let mut window_end = start + every;
    println!("time (UTC)  checks/s  allow   deny  chall.  errors    p50 ms    p99 ms  p99.9 ms    max ms  error codes");
    let mut received = 0;
    while let Some(outcome) = rx.recv().await {
        while Instant::now() >= window_end {
            window.report(every.as_secs_f64());
            window_end += every;
        }
        received += 1;
        match outcome {
            Ok((verdict, latency)) => {
                *window.verdicts.entry(verdict_name(verdict)).or_default() += 1;
                window.latencies.push(latency);
                all.push(latency);
            }
            Err(code) => {
                *window.errors.entry(format!("{code:?}")).or_default() += 1;
                errors += 1;
            }
        }
        if received == total {
            break;
        }
    }
    // The last window is usually part of one.
    let last = every.as_secs_f64() - window_end.saturating_duration_since(Instant::now()).as_secs_f64();
    window.report(last.max(1.0));
    sender.await?;
    all.sort();
    Ok((all, errors))
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Allow => "allow",
        Verdict::Deny => "deny",
        Verdict::Challenge => "challenge",
        Verdict::Unspecified => "unspecified",
    }
}

/// The checks answered in one window of an open-loop run.
#[derive(Default)]
struct Window {
    verdicts: BTreeMap<&'static str, usize>,
    errors: BTreeMap<String, usize>,
    latencies: Vec<Duration>,
}

impl Window {
    /// Prints the window's line and starts the next. A window with no answers prints too, so an
    /// outage shows as a run of lines with errors and nothing else.
    fn report(&mut self, seconds: f64) {
        self.latencies.sort();
        let ms = |p: f64| match self.latencies.is_empty() {
            true => "-".to_string(),
            false => format!("{:.2}", percentile(&self.latencies, p).as_secs_f64() * 1000.0),
        };
        let max = self.latencies.last().map_or("-".to_string(), |l| format!("{:.2}", l.as_secs_f64() * 1000.0));
        let count = |v: &str| self.verdicts.get(v).copied().unwrap_or(0);
        let codes: Vec<String> = self.errors.iter().map(|(code, n)| format!("{code} {n}")).collect();
        println!(
            "{}  {:8.0} {:6} {:6} {:7} {:7} {:>9} {:>9} {:>9} {:>9}  {}",
            chrono::Utc::now().format("%H:%M:%S"),
            self.latencies.len() as f64 / seconds,
            count("allow"),
            count("deny"),
            count("challenge"),
            self.errors.values().sum::<usize>(),
            ms(0.50),
            ms(0.99),
            ms(0.999),
            max,
            codes.join(", "),
        );
        *self = Self::default();
    }
}
