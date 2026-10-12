//! The PDP's hot path in one process, for measuring a change in seconds instead of a release:
//! `bazel run -c opt //authz/services/pdp:pdp_bench -- 8 10` (threads, seconds). Sends
//! pdp_loadtest's mix through what a gRPC check runs (the request's conversion and
//! `AppState::check`), with a principals snapshot as awzy-context would give, and no network.
//! On macOS, `sample <pid> 5` while it runs shows where the time goes.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use awzy_authz_proto::{
    authz::v1::{CheckRequest, Entity, EntityRef},
    json_to_struct,
};
use awzy_model::Check;
use awzy_pdp::{principals::Principals, AppState, Pdp};
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

/// Request `i` of pdp_loadtest's mix: humans and a delegated service account, across sensitivities.
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
    }
}

/// The principals as awzy-context's snapshot has them: people in nested groups, and the
/// registered service account.
fn snapshot() -> Vec<serde_json::Value> {
    let e = |kind: &str, id: &str, attrs: serde_json::Value, parents: &[&str]| {
        let parents: Vec<_> = parents.iter().map(|g| json!({ "type": "Group", "id": g })).collect();
        json!({ "uid": { "type": kind, "id": id }, "attrs": attrs, "parents": parents })
    };
    vec![
        e("Human", "alice", json!({}), &["security"]),
        e("Human", "bob", json!({}), &["engineering"]),
        e("ServiceAccount", "sales-genie", json!({ "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true }), &["engineering"]),
        e("Group", "security", json!({}), &["everyone"]),
        e("Group", "engineering", json!({}), &["everyone"]),
        e("Group", "everyone", json!({}), &[]),
    ]
}

async fn check(state: &AppState, i: usize) -> anyhow::Result<()> {
    let mut req = request(i);
    let entities = awzy_authz_proto::entities_to_json(std::mem::take(&mut req.entities));
    let access = Check::try_from(req)?;
    state.check(&access, entities).await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let threads: usize = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(8);
    let seconds: u64 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(10);

    let principals = Arc::new(Principals::awaiting_sync());
    principals.load(1, snapshot());
    let state = AppState::new(awzy_pdp::shared(Pdp::with_defaults()?)).with_principals(principals);
    for i in 0..1000 {
        check(&state, i).await?;
    }

    let done = Arc::new(AtomicUsize::new(0));
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let workers: Vec<_> = (0..threads)
        .map(|t| {
            let (state, done) = (state.clone(), done.clone());
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().build()?;
                let mut i = t;
                while Instant::now() < deadline {
                    runtime.block_on(check(&state, i))?;
                    done.fetch_add(1, Ordering::Relaxed);
                    i += threads;
                }
                anyhow::Ok(())
            })
        })
        .collect();
    for w in workers {
        w.join().map_err(|_| anyhow::anyhow!("a worker panicked"))??;
    }
    let checks = done.load(Ordering::Relaxed);
    println!("{threads} threads, {seconds}s, 1 in 3 delegated (two Cedar checks)");
    println!("checks   {checks}, {:.0}/s", checks as f64 / seconds as f64);
    println!("per check {:.1} µs of a thread", (threads as u64 * seconds) as f64 * 1e6 / checks as f64);
    Ok(())
}
