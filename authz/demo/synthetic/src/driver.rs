//! Drives Larkspur's activity ([`crate::activity`]) through awzy-gateway as real MCP tool calls
//! on larkspur-drive and larkspur-admin (demo plan step 5b), so every access is a PDP decision on
//! a real document or operation.
//!
//! Tokens are signed here with the gateway's development secret (`hs256_secret_env` in its
//! config): `sub` is the person, `act.sub` the agent acting for them, and an agent on its own
//! authority has `client_id` equal to `sub`, as the identity rules expect (ADR 0008, 0016). Only
//! for local and demo gateways; a client's gateway trusts only its own IdP.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::http::{header, Method, Request};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde_json::{json, Value};

use crate::activity::{server_of, Access};

type HttpClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>;

const TIMEOUT: Duration = Duration::from_secs(30);

/// Where the calls go and how their tokens are signed.
pub struct Driver {
    /// The gateway's MCP endpoints, before the upstream's name: e.g. http://127.0.0.1:8086/mcp.
    mcp: String,
    key: Signer,
    issuer: String,
    audience: String,
    http: HttpClient,
}

/// How the driver signs the tokens it presents.
enum Signer {
    Secret(EncodingKey),
    DevIdp,
}

/// What became of one tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// It ran. In monitor mode that includes calls the PDP would have blocked.
    Ran,
    /// The gateway blocked it: `deny`, `challenge`, or `unavailable`.
    Blocked(String),
    /// Anything else: the gateway or larkspur-drive failed, or the document wasn't found.
    Failed(String),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub ran: usize,
    pub blocked: usize,
    pub failed: usize,
}

impl Tally {
    pub fn add(&mut self, outcome: &Outcome) {
        match outcome {
            Outcome::Ran => self.ran += 1,
            Outcome::Blocked(_) => self.blocked += 1,
            Outcome::Failed(_) => self.failed += 1,
        }
    }
}

impl Driver {
    /// Signs tokens with the gateway's development secret (HS256).
    pub fn new(mcp: &str, secret: &[u8], issuer: &str, audience: &str) -> Self {
        Self::with_signer(mcp, Signer::Secret(EncodingKey::from_secret(secret)), issuer, audience)
    }

    /// Signs tokens as the dev IdP at `issuer`, for a gateway that trusts it (the demo install).
    pub fn signed_by_dev_idp(mcp: &str, issuer: &str, audience: &str) -> Self {
        Self::with_signer(mcp, Signer::DevIdp, issuer, audience)
    }

    fn with_signer(mcp: &str, key: Signer, issuer: &str, audience: &str) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https =
            hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
        Self {
            mcp: mcp.trim_end_matches('/').to_string(),
            key,
            issuer: issuer.to_string(),
            audience: audience.to_string(),
            http: Client::builder(TokioExecutor::new()).build(https),
        }
    }

    /// The bearer token the principal in `a` would present.
    pub fn token(&self, a: &Access) -> anyhow::Result<String> {
        let exp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + 300;
        let mut claims = json!({
            "iss": self.issuer, "aud": self.audience, "exp": exp,
            "groups": [a.team], "awzy_human_present": a.human_present,
        });
        match &a.on_behalf_of {
            Some(human) => {
                claims["sub"] = json!(human);
                claims["act"] = json!({ "sub": a.principal });
            }
            None => {
                claims["sub"] = json!(a.principal);
                if !a.human_present {
                    // RFC 9068: an application's own token.
                    claims["client_id"] = json!(a.principal);
                }
            }
        }
        match &self.key {
            Signer::Secret(key) => Ok(encode(&Header::default(), &claims, key)?),
            Signer::DevIdp => awzy_dev_idp::sign(&claims),
        }
    }

    /// Makes the tool call in `a` through the gateway.
    pub async fn send(&self, a: &Access, id: u64) -> Outcome {
        match self.try_send(a, id).await {
            Ok(outcome) => outcome,
            Err(e) => Outcome::Failed(e.to_string()),
        }
    }

    async fn try_send(&self, a: &Access, id: u64) -> anyhow::Result<Outcome> {
        let body = json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": a.tool, "arguments": { "id": a.resource } },
        });
        let req = Request::builder()
            .method(Method::POST)
            .uri(format!("{}/{}", self.mcp, server_of(a.tool)))
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token(a)?))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .body(Full::new(Bytes::from(body.to_string())))?;
        let res = tokio::time::timeout(TIMEOUT, self.http.request(req)).await.context("timed out")??;
        let status = res.status();
        let bytes = res.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            return Ok(Outcome::Failed(format!("{status}: {}", String::from_utf8_lossy(&bytes))));
        }
        let reply: Value = serde_json::from_slice(&bytes)?;
        Ok(classify(&reply))
    }
}

/// Reads the gateway's reply: a result, a tool error from awzy (with `awzy/verdict`), or another
/// failure.
fn classify(reply: &Value) -> Outcome {
    let result = &reply["result"];
    if let Some(error) = reply.get("error") {
        return Outcome::Failed(error["message"].as_str().unwrap_or("JSON-RPC error").to_string());
    }
    if result["isError"] != true {
        return Outcome::Ran;
    }
    match result["_meta"]["awzy/verdict"].as_str() {
        Some(verdict) => Outcome::Blocked(verdict.to_string()),
        None => Outcome::Failed(result["content"][0]["text"].as_str().unwrap_or("tool error").to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{http::HeaderMap, routing::post, Json, Router};
    use jsonwebtoken::{decode, DecodingKey, Validation};

    use super::*;
    use crate::activity::{Why, READ};

    fn access(principal: &str, on_behalf_of: Option<&str>, human_present: bool, resource: &str) -> Access {
        Access {
            minute: 9 * 60,
            principal: principal.into(),
            on_behalf_of: on_behalf_of.map(str::to_string),
            human_present,
            team: "sales-northeast-finserv",
            tool: READ,
            resource: resource.into(),
            why: Why::Assigned,
        }
    }

    fn claims(driver: &Driver, a: &Access) -> Value {
        let mut v = Validation::default();
        v.set_audience(&["awzy-gateway"]);
        decode::<Value>(&driver.token(a).unwrap(), &DecodingKey::from_secret(b"dev"), &v).unwrap().claims
    }

    #[test]
    fn tokens_say_who_acts_for_whom() {
        let d = Driver::new("http://unused", b"dev", "https://idp.local", "awzy-gateway");
        let nora = "nora.patel@larkspur.example";
        let human = claims(&d, &access(nora, None, true, "x"));
        assert_eq!((human["sub"].as_str(), human.get("act")), (Some(nora), None));
        assert_eq!(human["groups"], json!(["sales-northeast-finserv"]));

        let delegated = claims(&d, &access("sales-genie", Some(nora), true, "x"));
        assert_eq!((delegated["sub"].as_str(), delegated["act"]["sub"].as_str()), (Some(nora), Some("sales-genie")));

        let alone = claims(&d, &access("finance-summarizer", None, false, "x"));
        assert_eq!(alone["client_id"], "finance-summarizer");
        assert_eq!(alone["awzy_human_present"], false);
    }

    #[tokio::test]
    async fn sends_tool_calls_and_reads_what_the_gateway_said() {
        let seen: Arc<Mutex<Vec<(bool, Value)>>> = Arc::default();
        let log = seen.clone();
        let app = Router::new().route(
            "/mcp/larkspur-drive",
            post(move |headers: HeaderMap, Json(msg): Json<Value>| {
                let log = log.clone();
                async move {
                    log.lock().unwrap().push((headers.contains_key(header::AUTHORIZATION), msg.clone()));
                    let result = match msg["params"]["arguments"]["id"].as_str() {
                        Some("synthetic:doc:plan") => json!({ "isError": true, "_meta": { "awzy/verdict": "deny" },
                            "content": [{ "type": "text", "text": "awzy denied" }] }),
                        Some("synthetic:doc:missing") => json!({ "isError": true,
                            "content": [{ "type": "text", "text": "no document" }] }),
                        _ => json!({ "content": [{ "type": "text", "text": "Notes" }] }),
                    };
                    Json(json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result }))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let d = Driver::new(&format!("http://{addr}/mcp"), b"dev", "https://idp.local", "awzy-gateway");

        let nora = "nora.patel@larkspur.example";
        assert_eq!(d.send(&access(nora, None, true, "synthetic:doc:notes"), 1).await, Outcome::Ran);
        assert_eq!(d.send(&access(nora, None, true, "synthetic:doc:plan"), 2).await, Outcome::Blocked("deny".into()));
        assert!(matches!(d.send(&access(nora, None, true, "synthetic:doc:missing"), 3).await, Outcome::Failed(_)));

        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|(auth, _)| *auth), "every call carries a token");
        assert_eq!(
            seen[0].1["params"],
            json!({ "name": "read_document", "arguments": { "id": "synthetic:doc:notes" } })
        );

        let mut tally = Tally::default();
        for o in [Outcome::Ran, Outcome::Blocked("deny".into()), Outcome::Failed("x".into())] {
            tally.add(&o);
        }
        assert_eq!(tally, Tally { ran: 1, blocked: 1, failed: 1 });
    }
}
