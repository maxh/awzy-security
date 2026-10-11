//! Load test for a running PDP: `bazel run //authz/services/pdp:pdp_loadtest -- http://127.0.0.1:8082 32 20`
//! (endpoint, concurrent clients, seconds). Sends a mix of checks over gRPC, a third of them
//! delegated (two Cedar evaluations), and prints throughput and latency percentiles.
//!
//! Entities are sent inline, as enforcement points do until awzy-context exists.

use std::time::{Duration, Instant};

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
        context: Some(json_to_struct(json!({ "client": "loadtest" }).as_object().unwrap().clone())),
        on_behalf_of,
        entities,
        human_present: Some(true),
        always_enforced: false,
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

    let client = AuthorizerClient::connect(endpoint.clone()).await?;
    // Warm up connections and the PDP.
    for i in 0..200 {
        client.clone().check(request(i)).await?;
    }

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut workers = Vec::new();
    for w in 0..clients {
        let mut client = client.clone();
        workers.push(tokio::spawn(async move {
            let mut latencies = Vec::new();
            let mut errors = 0usize;
            let mut i = w;
            while Instant::now() < deadline {
                let start = Instant::now();
                match client.check(request(i)).await {
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
    all.sort();
    if all.is_empty() {
        anyhow::bail!("no successful checks ({errors} errors)");
    }
    println!("endpoint {endpoint}, {clients} clients, {seconds}s, 1 in 3 delegated (two Cedar checks)");
    println!("checks   {} ok, {errors} errors, {:.0}/s", all.len(), all.len() as f64 / seconds as f64);
    for (label, p) in [("p50", 0.50), ("p95", 0.95), ("p99", 0.99), ("p99.9", 0.999)] {
        println!("{label:8} {:.2} ms", percentile(&all, p).as_secs_f64() * 1000.0);
    }
    println!("max      {:.2} ms", all.last().unwrap().as_secs_f64() * 1000.0);
    Ok(())
}
