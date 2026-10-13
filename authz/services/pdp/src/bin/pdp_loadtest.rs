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
//! Entities are sent inline, as enforcement points do until awzy-context exists.
//!
//! In a cluster it runs from the PDP's image and sends the pod's service token for `awzy-pdp`
//! (ADR 0054), mounted as awzy's workloads mount theirs; without one it sends none.

use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use awzy_authz_proto::{
    authz::v1::{authorizer_client::AuthorizerClient, CheckRequest, Entity, EntityRef},
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

/// Request `i` of the mix: humans and a delegated service account, across sensitivities.
fn request(i: usize) -> CheckRequest {
    let resources = [("design-doc", "internal"), ("q3-strategy", "confidential"), ("board-deck", "crown_jewels")];
    let (res_id, sensitivity) = resources[i % resources.len()];
    let mut entities = vec![
        entity("Human", "alice", json!({}), &["security"]),
        entity("Human", "bob", json!({}), &["engineering"]),
        entity("Resource", res_id, json!({ "sensitivity": sensitivity }), &[]),
    ];
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
        Some(rate) => open_loop(connections, rate, seconds).await?,
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
    let ask = || {
        let mut client = client.clone();
        async move { client.check(request(i)).await }
    };
    awzy_service_auth::retry_once_if_unanswered(ask).await.is_ok()
}

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
async fn open_loop(connections: Vec<Client>, rate: f64, seconds: u64) -> anyhow::Result<(Vec<Duration>, usize)> {
    anyhow::ensure!(rate > 0.0, "the rate is in checks per second, above 0");
    let (tx, mut rx) = mpsc::unbounded_channel::<Option<Duration>>();
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
                let ok = check(&client, i).await;
                let _ = tx.send(ok.then(|| due.elapsed()));
            });
        }
    });

    // A line a minute: the checks answered in it, each timed from when it was due.
    let mut all = Vec::with_capacity(total);
    let mut window = Vec::new();
    let (mut errors, mut window_errors) = (0usize, 0usize);
    let mut window_end = start + Duration::from_secs(60);
    println!("minute   checks/s  errors    p50 ms    p99 ms  p99.9 ms    max ms");
    let mut minute = 1;
    while let Some(latency) = rx.recv().await {
        while Instant::now() >= window_end {
            report_minute(&mut window, &mut window_errors, minute, 60.0);
            window_end += Duration::from_secs(60);
            minute += 1;
        }
        match latency {
            Some(l) => {
                window.push(l);
                all.push(l);
            }
            None => {
                window_errors += 1;
                errors += 1;
            }
        }
        if all.len() + errors == total {
            break;
        }
    }
    // The last minute is usually part of one.
    let last = 60.0 - window_end.saturating_duration_since(Instant::now()).as_secs_f64();
    report_minute(&mut window, &mut window_errors, minute, last.max(1.0));
    sender.await?;
    all.sort();
    Ok((all, errors))
}

/// Prints one minute's line and starts the next.
fn report_minute(window: &mut Vec<Duration>, errors: &mut usize, minute: u64, seconds: f64) {
    window.sort();
    if let Some(max) = window.last() {
        println!(
            "{minute:6} {:10.0} {errors:7} {:9.2} {:9.2} {:9.2} {:9.2}",
            window.len() as f64 / seconds,
            percentile(window, 0.50).as_secs_f64() * 1000.0,
            percentile(window, 0.99).as_secs_f64() * 1000.0,
            percentile(window, 0.999).as_secs_f64() * 1000.0,
            max.as_secs_f64() * 1000.0,
        );
    }
    window.clear();
    *errors = 0;
}
