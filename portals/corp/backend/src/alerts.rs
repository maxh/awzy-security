//! Fleet alerts (ADR 0043): a message to Slack when an install becomes degraded, stops
//! reporting, or recovers.
//!
//! Every minute each install's health is compared with the last health it was alerted on. A
//! change has to hold for [`HOLD`] before it's sent, so a single flapping heartbeat doesn't page
//! anyone; "not reporting" already waits out the reporting timeout, so it's sent at once. Each
//! alert is claimed in Postgres before it's sent, so two corp portal tasks (during a deploy)
//! don't both send it.

use std::{future::Future, time::Duration as StdDuration};

use anyhow::Context;
use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use http_body_util::{BodyExt, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};

use crate::fleet::{FleetStore, Health, InstallRecord};

/// How long a new health has to hold before it's alerted on.
pub const HOLD: Duration = Duration::minutes(3);

/// How often installs are checked.
const EVERY: StdDuration = StdDuration::from_secs(60);

/// Where alerts go. A trait so the loop can be tested without Slack.
pub trait Notifier: Send + Sync + 'static {
    fn send(&self, text: &str) -> impl Future<Output = anyhow::Result<()>> + Send;
}

/// A Slack incoming webhook (AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL).
pub struct SlackWebhook {
    url: String,
    client: Client<hyper_rustls::HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl SlackWebhook {
    /// Only an `https://hooks.slack.com/` URL is accepted; anything else (such as the
    /// placeholder Terraform stores until someone sets the real one) turns alerts off.
    pub fn from_url(url: &str) -> Option<Self> {
        if !url.starts_with("https://hooks.slack.com/") {
            return None;
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let https = hyper_rustls::HttpsConnectorBuilder::new().with_webpki_roots().https_only().enable_http1().build();
        Some(Self { url: url.to_string(), client: Client::builder(TokioExecutor::new()).build(https) })
    }
}

impl Notifier for SlackWebhook {
    async fn send(&self, text: &str) -> anyhow::Result<()> {
        let body = serde_json::to_vec(&serde_json::json!({ "text": text }))?;
        let req = axum::http::Request::post(&self.url)
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(body)))?;
        let res = tokio::time::timeout(StdDuration::from_secs(10), self.client.request(req))
            .await
            .context("Slack didn't answer within 10s")??;
        let status = res.status();
        if !status.is_success() {
            let body = res.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
            anyhow::bail!("Slack answered {status}: {}", String::from_utf8_lossy(&body));
        }
        Ok(())
    }
}

/// What to do about one install this minute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing changed, or a change is still being held.
    Wait,
    /// Remember that `health` was first seen now; alert if it holds.
    Hold(Health),
    /// Record `health` as the install's known state without telling anyone: the first
    /// heartbeat, or a change that undid itself.
    Accept(Health),
    /// Tell staff the install went from `from` to `to`.
    Alert { from: Option<Health>, to: Health },
}

/// Decides what to do, from the install's health now, the last health it was alerted on (or
/// accepted), and the change being held, if any.
pub fn step(
    now: DateTime<Utc>,
    current: Health,
    alerted: Option<Health>,
    held: Option<(Health, DateTime<Utc>)>,
) -> Step {
    if current == Health::NeverReported {
        return Step::Wait;
    }
    if Some(current) == alerted {
        return if held.is_some() { Step::Accept(current) } else { Step::Wait };
    }
    // The first heartbeat of a healthy install isn't news.
    if alerted.is_none() && current == Health::Ok {
        return Step::Accept(current);
    }
    let hold = if current == Health::NotReporting { Duration::zero() } else { HOLD };
    match held {
        Some((h, since)) if h == current && now - since >= hold => Step::Alert { from: alerted, to: current },
        Some((h, _)) if h == current => Step::Wait,
        _ if hold.is_zero() => Step::Alert { from: alerted, to: current },
        _ => Step::Hold(current),
    }
}

/// Slack's mrkdwn treats these three characters as markup.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The alert's text. Install names come from the corp portal's registry; nothing in it comes
/// from inside an install except service names and replica counts (ADR 0009).
pub fn message(
    install: &InstallRecord,
    from: Option<Health>,
    to: Health,
    corp_url: Option<&str>,
    now: DateTime<Utc>,
) -> String {
    let name = format!("*{} / {}*", escape(&install.client), escape(&install.environment));
    let mut text = match to {
        Health::NotReporting => {
            let since = install
                .last_seen_at
                .map(|t| format!(" (last heartbeat {}m ago)", (now - t).num_minutes()))
                .unwrap_or_default();
            format!(":red_circle: {name} stopped reporting{since}.")
        }
        Health::Degraded => {
            let not_ready: Vec<String> = install
                .last_heartbeat
                .iter()
                .flat_map(|hb| hb.services.iter())
                .filter(|s| !s.ready || s.replicas_ready < s.replicas_desired)
                .map(|s| format!("{} {}/{} ready", s.service.as_str(), s.replicas_ready, s.replicas_desired))
                .collect();
            format!(":large_orange_circle: {name} is degraded: {}.", not_ready.join(", "))
        }
        Health::Ok => {
            let was = match from {
                Some(Health::NotReporting) => "reporting again",
                _ => "healthy again",
            };
            format!(":large_green_circle: {name} is {was}.")
        }
        Health::NeverReported => format!("{name} has never reported."),
    };
    if let Some(url) = corp_url {
        text.push_str(&format!(" <{url}/installs/{}|Open in the corp portal>", install.id));
    }
    text
}

/// Checks every install once.
pub async fn check<N: Notifier>(
    store: &FleetStore,
    notifier: Option<&N>,
    corp_url: Option<&str>,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    for install in store.all_installs().await? {
        let state = store.alert_state(install.id).await?;
        match step(now, install.health(now), state.alerted, state.held) {
            Step::Wait => {}
            Step::Hold(h) => store.hold_alert(install.id, h, now).await?,
            Step::Accept(h) => store.accept_health(install.id, h).await?,
            Step::Alert { from, to } => {
                // Claim it first: only the task whose update lands sends it.
                if !store.claim_alert(install.id, from, to).await? {
                    continue;
                }
                let text = message(&install, from, to, corp_url, now);
                tracing::warn!(install = %install.id, ?from, ?to, "fleet alert");
                if let Some(n) = notifier {
                    if let Err(e) = n.send(&text).await {
                        tracing::error!(error = %e, install = %install.id, "sending the alert failed; retrying next check");
                        store.unclaim_alert(install.id, from, to).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Checks every minute until the process ends. Without a notifier, transitions are only logged.
pub async fn run<N: Notifier>(store: FleetStore, notifier: Option<N>, corp_url: Option<String>) {
    if notifier.is_none() {
        tracing::warn!("fleet alerts aren't sent anywhere (AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL is unset or not a Slack webhook); transitions are only logged");
    }
    let mut tick = tokio::time::interval(EVERY);
    loop {
        tick.tick().await;
        if let Err(e) = check(&store, notifier.as_ref(), corp_url.as_deref(), Utc::now()).await {
            tracing::error!(error = %e, "checking installs for alerts failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn t(minutes: i64) -> DateTime<Utc> {
        "2026-10-09T12:00:00Z".parse::<DateTime<Utc>>().unwrap() + Duration::minutes(minutes)
    }

    #[test]
    fn a_healthy_first_heartbeat_is_accepted_quietly() {
        assert_eq!(step(t(0), Health::Ok, None, None), Step::Accept(Health::Ok));
        assert_eq!(step(t(0), Health::NeverReported, None, None), Step::Wait);
    }

    #[test]
    fn degraded_alerts_only_after_it_holds() {
        let ok = Some(Health::Ok);
        assert_eq!(step(t(0), Health::Degraded, ok, None), Step::Hold(Health::Degraded));
        assert_eq!(step(t(2), Health::Degraded, ok, Some((Health::Degraded, t(0)))), Step::Wait);
        assert_eq!(
            step(t(3), Health::Degraded, ok, Some((Health::Degraded, t(0)))),
            Step::Alert { from: ok, to: Health::Degraded }
        );
    }

    #[test]
    fn a_flap_that_undoes_itself_is_never_sent() {
        let ok = Some(Health::Ok);
        assert_eq!(step(t(1), Health::Ok, ok, Some((Health::Degraded, t(0)))), Step::Accept(Health::Ok));
    }

    #[test]
    fn not_reporting_alerts_at_once_and_recovery_holds() {
        let ok = Some(Health::Ok);
        assert_eq!(step(t(0), Health::NotReporting, ok, None), Step::Alert { from: ok, to: Health::NotReporting });
        let gone = Some(Health::NotReporting);
        assert_eq!(step(t(1), Health::Ok, gone, None), Step::Hold(Health::Ok));
        assert_eq!(step(t(4), Health::Ok, gone, Some((Health::Ok, t(1)))), Step::Alert { from: gone, to: Health::Ok });
    }

    #[derive(Default)]
    struct Recorded(Mutex<Vec<String>>);

    impl Notifier for Recorded {
        async fn send(&self, text: &str) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    fn beat(install: awzy_model::InstallId, ready: u32) -> awzy_heartbeat::Heartbeat {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "install_qid": install, "sent_at": Utc::now(), "chart_version": "0.5.0",
            "services": [{ "service": "awzy-pdp", "version": "0.5.0", "image_digest": null, "replicas_desired": 3,
                           "replicas_ready": ready, "restarts_24h": 0, "ready": true }],
            "metrics": {},
        }))
        .unwrap()
    }

    /// With --config=local-stack: an install degrades, holds, is alerted on once (by one of two
    /// checkers), stops reporting, and comes back.
    #[tokio::test]
    async fn alerts_follow_an_install_through_an_outage_once_each() {
        let Some(store) = crate::fleet::tests::test_store().await else { return };
        let (install, _) = store.register(&crate::fleet::tests::new_install("acme", "prod"), "max").await.unwrap();
        let slack = Recorded::default();
        let corp = Some("https://corp.example");

        store.record_heartbeat(&beat(install.id, 3), t(0)).await.unwrap();
        check(&store, Some(&slack), corp, t(0)).await.unwrap();
        store.record_heartbeat(&beat(install.id, 2), t(1)).await.unwrap();
        check(&store, Some(&slack), corp, t(1)).await.unwrap();
        assert!(slack.0.lock().unwrap().is_empty(), "still holding");
        store.record_heartbeat(&beat(install.id, 2), t(4)).await.unwrap();
        // Two corp portal tasks check at the same time; one sends.
        let (a, b) = tokio::join!(check(&store, Some(&slack), corp, t(4)), check(&store, Some(&slack), corp, t(4)));
        a.unwrap();
        b.unwrap();
        check(&store, Some(&slack), corp, t(5)).await.unwrap();
        {
            let sent = slack.0.lock().unwrap();
            assert_eq!(sent.len(), 1, "{sent:?}");
            assert!(sent[0].contains("*acme / prod* is degraded: awzy-pdp 2/3 ready."), "{}", sent[0]);
            assert!(sent[0].contains(&format!("<https://corp.example/installs/{}|", install.id)));
        }

        // No heartbeat for more than five minutes.
        check(&store, Some(&slack), corp, t(11)).await.unwrap();
        store.record_heartbeat(&beat(install.id, 3), t(12)).await.unwrap();
        check(&store, Some(&slack), corp, t(12)).await.unwrap();
        check(&store, Some(&slack), corp, t(15)).await.unwrap();
        let sent = slack.0.lock().unwrap();
        assert_eq!(sent.len(), 3, "{sent:?}");
        assert!(sent[1].contains("stopped reporting (last heartbeat 7m ago)"), "{}", sent[1]);
        assert!(sent[2].contains("is reporting again"), "{}", sent[2]);
    }
}
