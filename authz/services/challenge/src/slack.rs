//! The client's Slack, through the Slack app they installed in their workspace (ADR 0073): direct
//! messages to people, found by email, and posts to the security team's channel. Uses
//! `users.lookupByEmail` and `chat.postMessage`, so the app needs the `users:read.email` and
//! `chat:write` scopes.

use std::time::Duration;

use anyhow::{bail, Context};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde_json::{json, Value};

const TIMEOUT: Duration = Duration::from_secs(10);

type HttpClient = awzy_http::Client;

pub struct Slack {
    /// `https://slack.com/api`, or a stand-in in tests.
    api: String,
    token: String,
    /// The security team's channel ID.
    security_channel: String,
    http: HttpClient,
}

impl Slack {
    pub fn new(api: &str, token: &str, security_channel: &str) -> Self {
        Self {
            api: api.trim_end_matches('/').to_string(),
            token: token.to_string(),
            security_channel: security_channel.to_string(),
            http: awzy_http::client(),
        }
    }

    async fn call(&self, method: axum::http::Method, path: &str, body: Option<Value>) -> anyhow::Result<Value> {
        let mut req = axum::http::Request::builder()
            .method(method)
            .uri(format!("{}/{path}", self.api))
            .header("authorization", format!("Bearer {}", self.token));
        if body.is_some() {
            req = req.header("content-type", "application/json; charset=utf-8");
        }
        let req = req.body(Full::new(body.map(|b| Bytes::from(b.to_string())).unwrap_or_default()))?;
        let res = tokio::time::timeout(TIMEOUT, self.http.request(req)).await.context("Slack timed out")??;
        let status = res.status();
        let bytes = res.into_body().collect().await?.to_bytes();
        if !status.is_success() {
            bail!("Slack answered {status}");
        }
        let reply: Value = serde_json::from_slice(&bytes)?;
        // Slack reports failures in the body, with HTTP 200.
        if reply["ok"] != true {
            bail!("Slack {path}: {}", reply["error"].as_str().unwrap_or("not ok"));
        }
        Ok(reply)
    }

    /// A direct message to the person whose `userName` is `user_name`, which has to be their
    /// email address for Slack to find them.
    pub async fn to_human(&self, user_name: &str, text: &str) -> anyhow::Result<()> {
        if !user_name.contains('@') {
            bail!("{user_name} isn't an email address, so Slack can't find them");
        }
        let lookup = self
            .call(axum::http::Method::GET, &format!("users.lookupByEmail?email={}", encode(user_name)), None)
            .await?;
        let user = lookup["user"]["id"].as_str().context("Slack returned no user ID")?.to_string();
        self.post(&user, text).await
    }

    /// A post to the security team's channel.
    pub async fn to_security(&self, text: &str) -> anyhow::Result<()> {
        self.post(&self.security_channel.clone(), text).await
    }

    async fn post(&self, channel: &str, text: &str) -> anyhow::Result<()> {
        let body = json!({ "channel": channel, "text": text, "unfurl_links": false });
        self.call(axum::http::Method::POST, "chat.postMessage", Some(body)).await?;
        Ok(())
    }
}

/// Percent-encodes a query value.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        extract::Query,
        http::HeaderMap,
        routing::{get, post},
        Json, Router,
    };

    use super::*;

    async fn fake_slack(posts: Arc<Mutex<Vec<Value>>>) -> String {
        let app = Router::new()
            .route(
                "/users.lookupByEmail",
                get(|Query(q): Query<std::collections::HashMap<String, String>>| async move {
                    match q.get("email").map(String::as_str) {
                        Some("nora@larkspur.example") => Json(json!({ "ok": true, "user": { "id": "U123" } })),
                        _ => Json(json!({ "ok": false, "error": "users_not_found" })),
                    }
                }),
            )
            .route(
                "/chat.postMessage",
                post(move |headers: HeaderMap, Json(body): Json<Value>| {
                    let posts = posts.clone();
                    async move {
                        assert_eq!(headers["authorization"], "Bearer xoxb-test");
                        posts.lock().unwrap().push(body);
                        Json(json!({ "ok": true }))
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn people_get_direct_messages_and_the_security_team_its_channel() {
        let posts: Arc<Mutex<Vec<Value>>> = Arc::default();
        let slack = Slack::new(&fake_slack(posts.clone()).await, "xoxb-test", "C0SECURITY");
        slack.to_human("nora@larkspur.example", "awzy needs a reason").await.unwrap();
        slack.to_security("awzy asks for an approval").await.unwrap();

        let err = slack.to_human("someone@elsewhere.example", "x").await.unwrap_err();
        assert!(err.to_string().contains("users_not_found"), "{err}");
        assert!(slack.to_human("nora", "x").await.is_err(), "not an email address");

        let posts = posts.lock().unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(
            (posts[0]["channel"].as_str(), posts[0]["text"].as_str()),
            (Some("U123"), Some("awzy needs a reason"))
        );
        assert_eq!(posts[1]["channel"], "C0SECURITY");
    }
}
