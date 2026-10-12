//! Load for a gateway run locally, to measure and profile a tool call's whole path: token,
//! PDP check, forward, and answer. Serves a stub MCP upstream, mints HS256 tokens, and sends
//! `tools/call` from `clients` keep-alive connections for `seconds`:
//!
//! ```sh
//! bazel run -c opt //authz/services/gateway:gateway_bench -- serve 127.0.0.1:18090
//! AWZY_GATEWAY_BENCH_SECRET=s AWZY_GATEWAY_CONFIG=… awzy-gateway   # upstream http://127.0.0.1:18090/mcp
//! AWZY_GATEWAY_BENCH_SECRET=s gateway_bench load http://127.0.0.1:18081/mcp/drive 8 20
//! ```
//!
//! The gateway's config names issuer `https://idp.bench`, audience `awzy-gateway`, and the key
//! `{ "hs256_secret_env": "AWZY_GATEWAY_BENCH_SECRET" }`. Half the calls are by a person, half by
//! an AI service account acting for one; a third name a document, checked on it too.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use axum::{routing::post, Json, Router};
use serde_json::{json, Value};

const SECRET_ENV: &str = "AWZY_GATEWAY_BENCH_SECRET";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("serve") => serve(args.get(2).map_or("127.0.0.1:18090", String::as_str)).await,
        Some("load") => {
            let url = args.get(2).cloned().unwrap_or_else(|| "http://127.0.0.1:18081/mcp/drive".into());
            let clients: usize = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(8);
            let seconds: u64 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(10);
            load(url, clients, seconds).await
        }
        _ => anyhow::bail!("usage: gateway_bench serve <addr> | load <gateway url> <clients> <seconds>"),
    }
}

/// An MCP server that answers every tool call at once.
async fn serve(addr: &str) -> anyhow::Result<()> {
    let app = Router::new().route(
        "/mcp",
        post(|Json(msg): Json<Value>| async move {
            let tool = msg.pointer("/params/name").and_then(Value::as_str).unwrap_or("tool").to_string();
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            Json(json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": format!("ran {tool}") }] } }))
        }),
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("stub MCP upstream on http://{addr}/mcp");
    axum::serve(listener, app).await?;
    Ok(())
}

fn token(secret: &str, sub: &str, act: Option<&str>, group: &str) -> anyhow::Result<String> {
    let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() + 3600;
    // The person, and the service account acting for them when there is one.
    let actor = act.map(|account| json!({ "sub": account }));
    let claims = json!({ "iss": "https://idp.bench", "aud": "awzy-gateway", "sub": sub, "groups": [group], "exp": exp,
        "act": actor });
    let key = jsonwebtoken::EncodingKey::from_secret(secret.as_bytes());
    Ok(jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims, &key)?)
}

/// Call `i` of the mix.
fn call(i: usize) -> Value {
    let (name, arguments) = match i % 3 {
        0 => ("read_document", json!({ "id": "synthetic:doc:notes" })),
        1 => ("read_file", json!({ "path": "/notes.txt" })),
        _ => ("search", json!({ "query": "q3 plan" })),
    };
    json!({ "jsonrpc": "2.0", "id": i, "method": "tools/call", "params": { "name": name, "arguments": arguments } })
}

async fn load(url: String, clients: usize, seconds: u64) -> anyhow::Result<()> {
    let secret = std::env::var(SECRET_ENV).map_err(|_| anyhow::anyhow!("{SECRET_ENV} is not set"))?;
    let tokens = Arc::new([
        token(&secret, "alice", None, "security")?,
        token(&secret, "bob", Some("sales-genie"), "engineering")?,
    ]);
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let failed = Arc::new(AtomicUsize::new(0));
    let mut workers = Vec::new();
    for w in 0..clients {
        let (url, tokens, failed) = (url.clone(), tokens.clone(), failed.clone());
        workers.push(tokio::spawn(async move {
            // One keep-alive connection per client, as an agent's MCP session has.
            let client: awzy_http::Client = awzy_http::client();
            let mut latencies = Vec::new();
            let mut i = w;
            while Instant::now() < deadline {
                let started = Instant::now();
                let req = axum::http::Request::post(&url)
                    .header("content-type", "application/json")
                    .header("accept", "application/json, text/event-stream")
                    .header("authorization", format!("Bearer {}", tokens[i % 2]))
                    .body(http_body_util::Full::new(bytes::Bytes::from(call(i).to_string())));
                let ok = match req {
                    Ok(req) => match client.request(req).await {
                        Ok(res) => {
                            let ok = res.status().is_success();
                            ok && http_body_util::BodyExt::collect(res.into_body()).await.is_ok()
                        }
                        Err(_) => false,
                    },
                    Err(_) => false,
                };
                if ok {
                    latencies.push(started.elapsed());
                } else {
                    failed.fetch_add(1, Ordering::Relaxed);
                }
                i += clients;
            }
            latencies
        }));
    }
    let mut all = Vec::new();
    for w in workers {
        all.extend(w.await?);
    }
    all.sort();
    anyhow::ensure!(!all.is_empty(), "no successful calls ({} failed)", failed.load(Ordering::Relaxed));
    let at = |p: f64| all[((all.len() as f64 - 1.0) * p).round() as usize].as_secs_f64() * 1000.0;
    println!("{url}, {clients} clients, {seconds}s; a third also checked on a document");
    println!(
        "calls    {} ok, {} failed, {:.0}/s",
        all.len(),
        failed.load(Ordering::Relaxed),
        all.len() as f64 / seconds as f64
    );
    println!("p50 {:.2} ms, p95 {:.2} ms, p99 {:.2} ms, p99.9 {:.2} ms", at(0.5), at(0.95), at(0.99), at(0.999));
    Ok(())
}
