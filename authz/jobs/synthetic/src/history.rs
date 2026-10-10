//! A backdated month of Larkspur's activity, sent to awzy-intake as app events (demo plan step
//! 5c), so baselines and risk scores have history to start from instead of a month of waiting.
//!
//! The same seeded days as the live driver ([`crate::activity`]), placed on the workdays before
//! today: day 0 is the earliest. Each access is an event in awzy's own schema, from the
//! `larkspur-drive` or `larkspur-admin` system, as if it logged it. Why the generator chose each access (the
//! ground truth detection is scored against) is never sent: it's regenerated from the seed.

use std::time::Duration;

use anyhow::Context;
use axum::http::{header, Method, Request};
use bytes::Bytes;
use chrono::{DateTime, Datelike, Days, NaiveDate, TimeZone, Utc, Weekday};
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use serde_json::{json, Value};

use crate::{
    activity::{self, server_of, Access, ADMIN, LIST},
    org::{Company, Profile},
};

/// awzy-intake takes at most this many events per request.
pub const BATCH: usize = 1000;
const TIMEOUT: Duration = Duration::from_secs(30);

/// The `days` workdays (Monday to Friday) before `today`, earliest first.
pub fn workdays_before(today: NaiveDate, days: usize) -> Vec<NaiveDate> {
    let mut out = Vec::with_capacity(days);
    let mut d = today;
    while out.len() < days {
        d = d.checked_sub_days(Days::new(1)).expect("a date after year 1");
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            out.push(d);
        }
    }
    out.reverse();
    out
}

/// One access as an awzy-intake app event, on `date`. Larkspur's clock is UTC, for simplicity.
pub fn event(date: NaiveDate, a: &Access) -> Value {
    let at: DateTime<Utc> = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight"))
        + chrono::Duration::minutes(i64::from(a.minute));
    let subject_type = if a.on_behalf_of.is_some() || !a.human_present { "ServiceAccount" } else { "Human" };
    let system = server_of(a.tool);
    let mut e = json!({
        "at": at.to_rfc3339(),
        "system": system,
        "subject": { "type": subject_type, "id": a.subject },
        "action": if a.tool == LIST { "list" } else { "read" },
        "resource": { "type": "Resource", "id": a.resource },
        "outcome": "success",
        "attributes": { "tool": a.tool },
    });
    // An operation is on the tool, as the gateway checks it; what it operated on is an attribute.
    if system == ADMIN {
        e["action"] = json!(a.tool);
        e["resource"] = json!({ "type": "McpTool", "id": format!("{ADMIN}/{}", a.tool) });
        e["attributes"]["target"] = json!(a.resource);
    }
    if let Some(person) = &a.on_behalf_of {
        e["on_behalf_of"] = json!({ "type": "Human", "id": person });
    }
    e
}

/// Every event for the `days` workdays before `today`, a day at a time.
pub fn days(
    company: &Company,
    profile: Profile,
    today: NaiveDate,
    days: usize,
) -> impl Iterator<Item = Vec<Value>> + '_ {
    workdays_before(today, days)
        .into_iter()
        .enumerate()
        .map(move |(i, date)| activity::day(company, profile, i as u32).iter().map(|a| event(date, a)).collect())
}

/// awzy-intake at `base_url`, with the shippers' bearer token.
pub struct Intake {
    endpoint: String,
    token: String,
    http: Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl Intake {
    pub fn new(base_url: &str, token: &str) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https =
            hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_or_http().enable_http1().build();
        Self {
            endpoint: format!("{}/v1/events", base_url.trim_end_matches('/')),
            token: token.to_string(),
            http: Client::builder(TokioExecutor::new()).build(https),
        }
    }

    /// Sends `events` in batches awzy-intake accepts. Returns how many were acknowledged.
    pub async fn send(&self, events: &[Value]) -> anyhow::Result<usize> {
        for batch in events.chunks(BATCH) {
            let body = json!({ "events": batch }).to_string();
            let req = Request::builder()
                .method(Method::POST)
                .uri(&self.endpoint)
                .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Full::new(Bytes::from(body)))?;
            let res = tokio::time::timeout(TIMEOUT, self.http.request(req)).await.context("timed out")??;
            let status = res.status();
            let body = res.into_body().collect().await?.to_bytes();
            anyhow::ensure!(
                status.as_u16() == 202,
                "awzy-intake answered {status}: {}",
                String::from_utf8_lossy(&body)
            );
        }
        Ok(events.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{routing::post, Json, Router};

    use super::*;
    use crate::org::{cast, generate};

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn history_is_the_workdays_before_today() {
        // Friday 2026-10-09: the five days before it are Mon 10-05 to Thu 10-08 and Fri 10-02.
        let days = workdays_before(date("2026-10-09"), 5);
        assert_eq!(days, ["2026-10-02", "2026-10-05", "2026-10-06", "2026-10-07", "2026-10-08"].map(date));
        assert!(workdays_before(date("2026-10-12"), 30)
            .iter()
            .all(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)));
    }

    #[test]
    fn events_say_who_did_what_and_nothing_about_why() {
        let c = generate(Profile::Demo, 1);
        let day = activity::day(&c, Profile::Demo, 0);
        let nora = cast("sales-northeast-finserv").unwrap();
        let genie = day.iter().find(|a| a.subject == "sales-genie").unwrap();
        let e = event(date("2026-10-08"), genie);
        assert_eq!(e["subject"], json!({ "type": "ServiceAccount", "id": "sales-genie" }));
        assert_eq!(e["on_behalf_of"], json!({ "type": "Human", "id": nora }));
        assert!(e["at"].as_str().unwrap().starts_with("2026-10-08T"));
        let person = day.iter().find(|a| a.on_behalf_of.is_none() && a.human_present).unwrap();
        let p = event(date("2026-10-08"), person);
        assert_eq!(p["subject"]["type"], "Human");
        assert!(p.get("on_behalf_of").is_none());
        let alone = day.iter().find(|a| a.subject == "finance-summarizer").unwrap();
        assert_eq!(event(date("2026-10-08"), alone)["subject"]["type"], "ServiceAccount");
        let op = day.iter().find(|a| a.why == activity::Why::Admin).unwrap();
        let e = event(date("2026-10-08"), op);
        assert_eq!((e["system"].as_str(), e["action"].as_str()), (Some("larkspur-admin"), Some(op.tool)));
        assert_eq!(e["resource"], json!({ "type": "McpTool", "id": format!("larkspur-admin/{}", op.tool) }));
        assert_eq!(e["attributes"]["target"], json!(op.resource));
        for e in day.iter().map(|a| event(date("2026-10-08"), a)) {
            assert!(e.get("why").is_none() && e["attributes"].get("why").is_none(), "the ground truth stays here");
        }
    }

    #[tokio::test]
    async fn sends_in_batches_intake_accepts_with_its_token() {
        let seen: Arc<Mutex<Vec<(String, usize)>>> = Arc::default();
        let log = seen.clone();
        let app = Router::new().route(
            "/v1/events",
            post(move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                let log = log.clone();
                async move {
                    let auth = headers.get(header::AUTHORIZATION).unwrap().to_str().unwrap().to_string();
                    log.lock().unwrap().push((auth, body["events"].as_array().unwrap().len()));
                    (axum::http::StatusCode::ACCEPTED, Json(json!({ "accepted": 0 })))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let c = generate(Profile::Demo, 1);
        let events: Vec<Value> = days(&c, Profile::Demo, date("2026-10-09"), 1).flatten().collect();
        assert!(events.len() > BATCH, "a demo day is more than one batch: {}", events.len());
        let sent = Intake::new(&format!("http://{addr}"), "shipper-token").send(&events).await.unwrap();
        assert_eq!(sent, events.len());
        let seen = seen.lock().unwrap();
        assert!(seen.iter().all(|(auth, n)| auth == "Bearer shipper-token" && *n <= BATCH));
        assert_eq!(seen.iter().map(|(_, n)| n).sum::<usize>(), events.len());
    }
}
