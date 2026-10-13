//! Load test for a running PDP: `bazel run //authz/services/pdp:pdp_loadtest -- http://127.0.0.1:8082 32 20`
//! (endpoint, concurrent clients, seconds). Sends a mix of checks over gRPC, a third of them
//! delegated (two Cedar evaluations), and prints throughput and latency percentiles. Each client
//! has its own connection.
//!
//! A fourth argument, a number of seconds, also prints a line for each interval as it ends: its
//! verdicts, its errors by gRPC code, and its p99. A game day reads these to time what a PDP does
//! while a dependency is away (#353), so a check that doesn't answer within a second counts as an
//! error, as an enforcement point would give up on it.
//!
//! Entities are sent inline, as enforcement points do until awzy-context exists.
//!
//! In a cluster it runs from the PDP's image and sends the pod's service token for `awzy-pdp`
//! (ADR 0054), mounted as awzy's workloads mount theirs; without one it sends none.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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

/// How long a check may take in timeline mode before it counts as an error.
const TIMELINE_CHECK_TIMEOUT: Duration = Duration::from_secs(1);

/// One interval of a timeline: what the checks that finished in it got back.
#[derive(Default)]
struct Interval {
    verdicts: BTreeMap<&'static str, usize>,
    errors: BTreeMap<String, usize>,
    latencies: Vec<Duration>,
}

impl Interval {
    fn line(mut self, at: chrono::DateTime<chrono::Utc>) -> String {
        self.latencies.sort();
        let ok: usize = self.verdicts.values().sum();
        let verdicts: Vec<String> = self.verdicts.iter().map(|(v, n)| format!("{v} {n}")).collect();
        let errors: Vec<String> = self.errors.iter().map(|(c, n)| format!("{c} {n}")).collect();
        let p99 = match self.latencies.is_empty() {
            true => "-".to_string(),
            false => format!("{:.2} ms", percentile(&self.latencies, 0.99).as_secs_f64() * 1000.0),
        };
        format!(
            "{}  ok {ok} ({})  errors {} ({})  p99 {p99}",
            at.format("%H:%M:%S"),
            verdicts.join(", "),
            self.errors.values().sum::<usize>(),
            errors.join(", "),
        )
    }
}

fn verdict_name(verdict: i32) -> &'static str {
    match Verdict::try_from(verdict) {
        Ok(Verdict::Allow) => "allow",
        Ok(Verdict::Deny) => "deny",
        Ok(Verdict::Challenge) => "challenge",
        _ => "unspecified",
    }
}

/// Checks from every client, grouped by the interval they finished in.
struct Timeline {
    every: Duration,
    started: Instant,
    intervals: Mutex<BTreeMap<u64, Interval>>,
}

impl Timeline {
    fn index(&self, at: Instant) -> u64 {
        (at.duration_since(self.started).as_millis() / self.every.as_millis().max(1)) as u64
    }

    fn record(
        &self,
        result: &Result<tonic::Response<awzy_authz_proto::authz::v1::CheckResponse>, tonic::Status>,
        latency: Duration,
    ) {
        let index = self.index(Instant::now());
        let mut intervals = self.intervals.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let interval = intervals.entry(index).or_default();
        match result {
            Ok(response) => {
                *interval.verdicts.entry(verdict_name(response.get_ref().verdict)).or_default() += 1;
                interval.latencies.push(latency);
            }
            Err(status) => *interval.errors.entry(format!("{:?}", status.code()).to_lowercase()).or_default() += 1,
        }
    }

    /// Prints each interval once it has ended, until `deadline`.
    async fn report(self: Arc<Self>, deadline: Instant) {
        let mut next = 0;
        loop {
            tokio::time::sleep_until((self.started + self.every * (next as u32 + 1)).into()).await;
            // A check still in flight lands in the interval it finishes in, which has ended by now
            // only if it took longer than the timeout.
            let interval = self.intervals.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&next);
            let ended = chrono::Utc::now();
            println!("{}", interval.unwrap_or_default().line(ended));
            next += 1;
            if Instant::now() >= deadline {
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let endpoint = args.get(1).cloned().unwrap_or_else(|| "http://127.0.0.1:8082".into());
    let clients: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(16);
    let seconds: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(10);
    let every: Option<u64> = args.get(4).map(|s| s.parse()).transpose()?;

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

    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let timeline = every
        .map(|every| Arc::new(Timeline { every: Duration::from_secs(every), started, intervals: Mutex::default() }));
    let reporter = timeline.clone().map(|timeline| tokio::spawn(timeline.report(deadline)));
    let mut workers = Vec::new();
    for (w, client) in connections.into_iter().enumerate() {
        let timeline = timeline.clone();
        workers.push(tokio::spawn(async move {
            let mut latencies = Vec::new();
            let mut errors = 0usize;
            let mut i = w;
            while Instant::now() < deadline {
                let start = Instant::now();
                // Retried once like an enforcement point does, so a rollout counts what they'd see.
                let ask = || {
                    let mut client = client.clone();
                    async move { client.check(request(i)).await }
                };
                let result = match timeline {
                    Some(_) => {
                        tokio::time::timeout(TIMELINE_CHECK_TIMEOUT, awzy_service_auth::retry_once_if_unanswered(ask))
                            .await
                            .unwrap_or_else(|_| Err(tonic::Status::deadline_exceeded("no answer within a second")))
                    }
                    None => awzy_service_auth::retry_once_if_unanswered(ask).await,
                };
                if let Some(timeline) = &timeline {
                    timeline.record(&result, start.elapsed());
                }
                match result {
                    Ok(_) => latencies.push(start.elapsed()),
                    Err(_) => errors += 1,
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
    if let Some(reporter) = reporter {
        reporter.await?;
    }
    all.sort();
    if all.is_empty() {
        anyhow::bail!("no successful checks ({errors} errors)");
    }
    println!("endpoint {endpoint}, {clients} clients, {seconds}s, 1 in 3 delegated (two Cedar checks)");
    println!("checks   {} ok, {errors} errors, {:.0}/s", all.len(), all.len() as f64 / seconds as f64);
    for (label, p) in [("p50", 0.50), ("p95", 0.95), ("p99", 0.99), ("p99.9", 0.999)] {
        println!("{label:8} {:.2} ms", percentile(&all, p).as_secs_f64() * 1000.0);
    }
    println!("max      {:.2} ms", all.last().copied().unwrap_or_default().as_secs_f64() * 1000.0);
    Ok(())
}
