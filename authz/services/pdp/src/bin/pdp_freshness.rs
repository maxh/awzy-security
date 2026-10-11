//! Measures the platform plan's freshness targets on running services, under load: how long from a
//! write until the PDP's decisions reflect it.
//!
//! `bazel run -c opt //authz/services/pdp:pdp_freshness -- [rounds] [load clients]`, with
//! AWZY_PDP_ADDR, AWZY_POLICY_ADDR, AWZY_CHALLENGE_ADDR, and AWZY_CONTEXT_URL plus AWZY_SCIM_TOKEN
//! (the PDP must be configured with the same awzy-policy, awzy-challenge, and awzy-context). For
//! test installs only: it publishes a bundle (the active one plus probe policies on its own probe
//! resources), provisions a probe person and group over SCIM, contains and lifts the probe person,
//! and republishes the original bundle at the end.
//!
//! Each target is timed from the write to the first decision that reflects it, and to the start of
//! 20 reflecting decisions in a row ("settled"), which with several PDP replicas behind a Service
//! approximates every PDP having it.

use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use awzy_authz_proto::{
    authz::v1::{authorizer_client::AuthorizerClient, Actor, CheckRequest, CheckResponse, Entity, EntityRef, Verdict},
    challenge::v1::{challenge_service_client::ChallengeServiceClient, ContainRequest, JustifyRequest, LiftRequest},
    json_to_struct,
    policy::v1::{policy_service_client::PolicyServiceClient, CreateDraftRequest, GetActiveRequest, PublishRequest},
};
use awzy_service_auth::{grpc_channel, AuthChannel};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::{json, Value};
use tonic::transport::Channel;

const PROBE: &str = "freshness-probe@awzy.local";
const PROBE_GROUP: &str = "freshness-probe-group";
/// Consecutive reflecting decisions that count as settled.
const SETTLED: usize = 20;
const POLL: Duration = Duration::from_millis(10);
const GIVE_UP: Duration = Duration::from_secs(120);

fn r(kind: &str, id: &str) -> EntityRef {
    EntityRef { r#type: kind.into(), id: id.into() }
}

/// Who the harness's writes are by (ADR 0060).
fn me() -> Actor {
    Actor { subject: Some(r("AwzyService", "pdp-freshness")), on_behalf_of: None }
}

/// A probe resource under `freshness:root`, with its folders, sent inline: awzy-context doesn't
/// know them, so the PDP keeps these (ADR 0044).
fn probe_entities(resource: &str, folder: &str) -> Vec<Entity> {
    let res = |id: &str, parents: &[&str]| Entity {
        uid: Some(r("Resource", id)),
        attrs: Some(json_to_struct(json!({ "sensitivity": "internal" }).as_object().unwrap().clone())),
        parents: parents.iter().map(|p| r("Resource", p)).collect(),
    };
    vec![res(resource, &[folder]), res(folder, &["freshness:root"]), res("freshness:root", &[])]
}

fn probe_check(resource: &str, folder: &str) -> CheckRequest {
    CheckRequest {
        principal: Some(r("Human", PROBE)),
        action: "read".into(),
        resource: Some(r("Resource", resource)),
        entities: probe_entities(resource, folder),
        human_present: Some(true),
        // Opens challenges whatever the install's mode, as the portal's checks do.
        always_enforced: true,
        ..Default::default()
    }
}

/// The probe policies added to the active bundle. They name only the probe person, group, and
/// resources, so they don't change any other decision.
fn probe_policies() -> String {
    format!(
        r#"
@id("freshness-probe-read")
permit (principal == Human::"{PROBE}", action == Action::"read", resource in Resource::"freshness:root");

@id("freshness-probe-challenge")
@challenge("justification")
forbid (principal == Human::"{PROBE}", action == Action::"read", resource in Resource::"freshness:challenges");

@id("freshness-probe-group")
permit (principal in Group::"{PROBE_GROUP}", action == Action::"read", resource in Resource::"freshness:group");

@id("freshness-probe-public")
permit (principal == Human::"{PROBE}", action == Action::"read", resource == Resource::"freshness:labeled")
when {{ resource.sensitivity == "public" }};

@id("freshness-probe-group-only")
forbid (principal == Human::"{PROBE}", action == Action::"read", resource in Resource::"freshness:group")
unless {{ principal in Group::"{PROBE_GROUP}" }};
"#
    )
}

/// How long one change took to show.
#[derive(Debug, Clone, Copy)]
struct Timing {
    first: Duration,
    settled: Duration,
}

/// Polls `check` until `reflects` holds for SETTLED decisions in a row, timing from `since`.
async fn time_until(
    pdp: &mut AuthorizerClient<AuthChannel>,
    req: &CheckRequest,
    since: Instant,
    reflects: impl Fn(&CheckResponse) -> bool,
) -> anyhow::Result<Timing> {
    let mut first = None;
    let mut run_start = None;
    let mut run = 0;
    loop {
        let res = pdp.check(req.clone()).await?.into_inner();
        let now = since.elapsed();
        if reflects(&res) {
            first.get_or_insert(now);
            run_start.get_or_insert(now);
            run += 1;
            if run >= SETTLED {
                return Ok(Timing { first: first.unwrap(), settled: run_start.unwrap() });
            }
        } else {
            run = 0;
            run_start = None;
        }
        if now > GIVE_UP {
            bail!("no reflecting decision after {GIVE_UP:?}: last was {:?}", res.verdict());
        }
        tokio::time::sleep(POLL).await;
    }
}

fn verdict(v: Verdict) -> impl Fn(&CheckResponse) -> bool {
    move |res| res.verdict() == v
}

struct Scim {
    base: String,
    token: String,
    http: Client<HttpConnector, Full<Bytes>>,
}

impl Scim {
    async fn send(&self, method: &str, path: &str, body: Option<Value>) -> anyhow::Result<(u16, Value)> {
        let req = axum::http::Request::builder()
            .method(method)
            .uri(format!("{}/scim/v2{path}", self.base))
            .header("authorization", format!("Bearer {}", self.token))
            .header("content-type", "application/scim+json")
            .body(Full::new(Bytes::from(body.map(|b| b.to_string()).unwrap_or_default())))?;
        let res = self.http.request(req).await?;
        let status = res.status().as_u16();
        let bytes = res.into_body().collect().await?.to_bytes();
        Ok((status, serde_json::from_slice(&bytes).unwrap_or(Value::Null)))
    }

    /// The SCIM id of the user or group named `name`, created if it doesn't exist.
    async fn ensure(&self, kind: &str, attr: &str, name: &str) -> anyhow::Result<String> {
        let filter = format!("/{kind}?filter={attr}%20eq%20%22{}%22", name.replace('@', "%40"));
        let (_, found) = self.send("GET", &filter, None).await?;
        if let Some(id) = found["Resources"][0]["id"].as_str() {
            return Ok(id.to_string());
        }
        let body =
            if kind == "Users" { json!({ "userName": name, "active": true }) } else { json!({ "displayName": name }) };
        let (status, created) = self.send("POST", &format!("/{kind}"), Some(body)).await?;
        created["id"].as_str().map(str::to_string).with_context(|| format!("creating {name}: {status} {created}"))
    }

    /// Writes the probe document to awzy-context, labeled `sensitivity`, as a connector would.
    async fn label(&self, sensitivity: &str) -> anyhow::Result<()> {
        let req = axum::http::Request::builder()
            .method("PUT")
            .uri(format!("{}/v1/resources/freshness:labeled", self.base))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(
                json!({ "source": "freshness", "kind": "document", "name": "Freshness probe", "sensitivity": sensitivity })
                    .to_string(),
            )))?;
        let res = self.http.request(req).await?;
        anyhow::ensure!(res.status().is_success(), "labeling the probe document: {}", res.status());
        Ok(())
    }

    async fn membership(&self, group: &str, user: &str, add: bool) -> anyhow::Result<()> {
        let op = if add { "add" } else { "remove" };
        let body = json!({ "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [{ "op": op, "path": "members", "value": [{ "value": user }] }] });
        let (status, res) = self.send("PATCH", &format!("/Groups/{group}"), Some(body)).await?;
        anyhow::ensure!((200..300).contains(&status), "changing {group}: {status} {res}");
        Ok(())
    }
}

/// Background load: `clients` checking continuously until told to stop. Returns checks and p99.
fn background(
    pdp: AuthorizerClient<AuthChannel>,
    clients: usize,
    stop: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<(usize, Duration)> {
    tokio::spawn(async move {
        let mut workers = Vec::new();
        for w in 0..clients {
            let mut pdp = pdp.clone();
            let stop = stop.clone();
            workers.push(tokio::spawn(async move {
                let mut latencies = Vec::new();
                let mut i = w;
                while !*stop.borrow() {
                    let human = if i % 2 == 0 { "alice" } else { "bob" };
                    let req = CheckRequest {
                        principal: Some(r("Human", human)),
                        action: "read".into(),
                        resource: Some(r("Resource", &format!("load:{}", i % 50))),
                        entities: vec![Entity {
                            uid: Some(r("Resource", &format!("load:{}", i % 50))),
                            attrs: Some(json_to_struct(
                                json!({ "sensitivity": "internal" }).as_object().unwrap().clone(),
                            )),
                            parents: vec![],
                        }],
                        human_present: Some(true),
                        ..Default::default()
                    };
                    let start = Instant::now();
                    if pdp.check(req).await.is_ok() {
                        latencies.push(start.elapsed());
                    }
                    i += clients;
                }
                latencies
            }));
        }
        let mut all = Vec::new();
        for w in workers {
            all.extend(w.await.unwrap_or_default());
        }
        all.sort();
        let p99 = all.get(((all.len() as f64 - 1.0) * 0.99).round() as usize).copied().unwrap_or_default();
        (all.len(), p99)
    })
}

fn report(name: &str, target: &str, timings: &[Timing]) {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let mut first: Vec<Duration> = timings.iter().map(|t| t.first).collect();
    let mut settled: Vec<Duration> = timings.iter().map(|t| t.settled).collect();
    first.sort();
    settled.sort();
    let mid = |v: &[Duration]| v[v.len() / 2];
    println!(
        "{name:28} target {target:>6}   first: median {:>8.1} ms, max {:>8.1} ms   settled: median {:>8.1} ms, max {:>8.1} ms",
        ms(mid(&first)),
        ms(*first.last().unwrap()),
        ms(mid(&settled)),
        ms(*settled.last().unwrap()),
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let rounds: usize = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(5);
    let clients: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(16);
    let var = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
    let channel = |addr: String, audience: &str| -> anyhow::Result<AuthChannel> {
        Ok(grpc_channel(Channel::from_shared(addr)?.connect_lazy(), audience))
    };
    let mut pdp = AuthorizerClient::new(channel(var("AWZY_PDP_ADDR")?, "awzy-pdp")?);
    let mut policy = PolicyServiceClient::new(channel(var("AWZY_POLICY_ADDR")?, "awzy-policy")?);
    let mut challenge = ChallengeServiceClient::new(channel(var("AWZY_CHALLENGE_ADDR")?, "awzy-challenge")?);
    let scim = Scim {
        base: var("AWZY_CONTEXT_URL")?.trim_end_matches('/').to_string(),
        token: var("AWZY_SCIM_TOKEN")?,
        http: Client::builder(TokioExecutor::new()).build_http(),
    };

    // The probe person and group, the person not yet in the group; then wait until the PDP's
    // snapshot has the person, so the probe policies' principal is known.
    let user = scim.ensure("Users", "userName", PROBE).await?;
    let group = scim.ensure("Groups", "displayName", PROBE_GROUP).await?;
    scim.membership(&group, &user, false).await?;

    let original = policy
        .get_active(GetActiveRequest { known_version: 0, wait_seconds: 0 })
        .await?
        .into_inner()
        .bundle
        .context("no active bundle")?;
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let load = background(pdp.clone(), clients, stop_rx);
    println!("{clients} background clients checking; {rounds} rounds of each measurement");

    let result = async {
        // 1. A published bundle: the probe's challenge policy starts to fire.
        let mut bundle = Vec::new();
        for round in 0..rounds {
            // Alternate: with the probe policies, then without, so each round is a real change.
            let with_probe = round % 2 == 0;
            let policies = if with_probe {
                format!("{}\n{}", original.policies, probe_policies())
            } else {
                original.policies.clone()
            };
            let draft = policy
                .create_draft(CreateDraftRequest {
                    schema: original.schema.clone(),
                    policies,
                    created_by_actor: Some(me()),
                    ..Default::default()
                })
                .await?
                .into_inner()
                .bundle
                .context("no draft")?;
            let req = probe_check("freshness:challenges:bundle", "freshness:challenges");
            let since = Instant::now();
            policy
                .publish(PublishRequest {
                    policy_bundle_qid: draft.qid,
                    published_by_actor: Some(me()),
                    ..Default::default()
                })
                .await?;
            let expect = if with_probe { Verdict::Challenge } else { Verdict::Deny };
            bundle.push(time_until(&mut pdp, &req, since, verdict(expect)).await?);
        }
        // Leave the probe policies in force for the rest.
        if rounds.is_multiple_of(2) {
            let draft = policy
                .create_draft(CreateDraftRequest {
                    schema: original.schema.clone(),
                    policies: format!("{}\n{}", original.policies, probe_policies()),
                    created_by_actor: Some(me()),
                    ..Default::default()
                })
                .await?
                .into_inner()
                .bundle
                .context("no draft")?;
            policy
                .publish(PublishRequest {
                    policy_bundle_qid: draft.qid,
                    published_by_actor: Some(me()),
                    ..Default::default()
                })
                .await?;
            time_until(
                &mut pdp,
                &probe_check("freshness:challenges:x", "freshness:challenges"),
                Instant::now(),
                verdict(Verdict::Challenge),
            )
            .await?;
        }

        // 2. A passed challenge: a new probe resource each round, so each is a new challenge.
        let mut passed = Vec::new();
        for round in 0..rounds {
            let req =
                probe_check(&format!("freshness:challenges:{}:{round}", std::process::id()), "freshness:challenges");
            let res = pdp.check(req.clone()).await?.into_inner();
            if res.verdict() != Verdict::Challenge || res.challenge_qid.is_empty() {
                bail!(
                    "expected an open challenge, got {:?} (is the PDP configured with awzy-challenge?)",
                    res.verdict()
                );
            }
            let since = Instant::now();
            challenge
                .justify(JustifyRequest {
                    challenge_qid: res.challenge_qid,
                    answered_by_actor: Some(Actor { subject: Some(r("Human", PROBE)), on_behalf_of: None }),
                    justification: "Measuring how fast a passed challenge is honored".into(),
                    ..Default::default()
                })
                .await?;
            passed.push(time_until(&mut pdp, &req, since, verdict(Verdict::Allow)).await?);
        }

        // 3. A containment, and its lift.
        let probe = probe_check("freshness:plain", "freshness:plain-folder");
        let (mut contained, mut lifted) = (Vec::new(), Vec::new());
        for _ in 0..rounds {
            let since = Instant::now();
            let c = challenge
                .contain(ContainRequest {
                    principal: Some(r("Human", PROBE)),
                    reason: "freshness measurement".into(),
                    created_by_actor: Some(me()),
                    ..Default::default()
                })
                .await?
                .into_inner()
                .containment
                .context("no containment")?;
            contained.push(time_until(&mut pdp, &probe, since, verdict(Verdict::Deny)).await?);
            let since = Instant::now();
            challenge
                .lift(LiftRequest {
                    containment_qid: c.qid,
                    lifted_by_actor: Some(me()),
                    reason: "measured".into(),
                    ..Default::default()
                })
                .await?;
            lifted.push(time_until(&mut pdp, &probe, since, verdict(Verdict::Allow)).await?);
        }

        // 4. An attribute change: the probe joins and leaves a group in the IdP.
        let group_probe = probe_check("freshness:group:doc", "freshness:group");
        let (mut joined, mut left) = (Vec::new(), Vec::new());
        for _ in 0..rounds {
            let since = Instant::now();
            scim.membership(&group, &user, true).await?;
            joined.push(time_until(&mut pdp, &group_probe, since, verdict(Verdict::Allow)).await?);
            let since = Instant::now();
            scim.membership(&group, &user, false).await?;
            left.push(time_until(&mut pdp, &group_probe, since, verdict(Verdict::Deny)).await?);
        }
        // 5. A resource attribute: the probe document relabeled in awzy-context. The PDP loads it
        // by ID, with no inline entity, through its resource cache (ADR 0044).
        let labeled = CheckRequest {
            principal: Some(r("Human", PROBE)),
            action: "read".into(),
            resource: Some(r("Resource", "freshness:labeled")),
            human_present: Some(true),
            always_enforced: true,
            ..Default::default()
        };
        scim.label("confidential").await?;
        time_until(&mut pdp, &labeled, Instant::now(), verdict(Verdict::Deny)).await?;
        let mut relabeled = Vec::new();
        for round in 0..rounds {
            let public = round % 2 == 0;
            let since = Instant::now();
            scim.label(if public { "public" } else { "confidential" }).await?;
            relabeled.push(
                time_until(&mut pdp, &labeled, since, verdict(if public { Verdict::Allow } else { Verdict::Deny }))
                    .await?,
            );
        }
        anyhow::Ok((bundle, passed, contained, lifted, joined, left, relabeled))
    }
    .await;

    // Put the original policies back, whatever happened.
    let restored = policy
        .create_draft(CreateDraftRequest {
            schema: original.schema.clone(),
            policies: original.policies.clone(),
            created_by_actor: Some(me()),
            ..Default::default()
        })
        .await?
        .into_inner()
        .bundle
        .context("no draft")?;
    policy
        .publish(PublishRequest {
            policy_bundle_qid: restored.qid,
            published_by_actor: Some(me()),
            ..Default::default()
        })
        .await?;
    stop_tx.send(true)?;
    let (checks, p99) = load.await?;

    let (bundle, passed, contained, lifted, joined, left, relabeled) = result?;
    println!();
    report("published bundle live", "60 s", &bundle);
    report("passed challenge honored", "2 s", &passed);
    report("containment applied", "2 s", &contained);
    report("containment lifted", "-", &lifted);
    report("attribute change (joined)", "60 s", &joined);
    report("attribute change (left)", "60 s", &left);
    report("resource relabeled", "60 s", &relabeled);
    println!("\nbackground: {checks} checks, p99 {:.2} ms", p99.as_secs_f64() * 1000.0);
    Ok(())
}
