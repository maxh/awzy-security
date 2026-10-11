//! awzy-reporter: sends this install's heartbeat to the corp portal every minute (ADR 0026).
//!
//! The heartbeat is the only thing that leaves an install for awzy (ADR 0009), in the fixed
//! schema of `awzy_heartbeat`. Every heartbeat is logged before it is sent, `--print` shows one
//! without sending it, and AWZY_REPORTER_ENABLED=false turns reporting off. The last one sent is
//! served at `/v1/last-heartbeat`, so the client portal can show exactly what left.
//!
//! In a cluster it reads the awzy workloads from the Kubernetes API (replicas, restarts, image
//! digests) and scrapes each PDP pod's `/metrics` for decisions per second, p99, and error rate,
//! and the PDP, gateway, and connector pods' for freshness ([`freshness`]). Locally it probes each
//! service's `/readyz` and `/version`, and those services' `/metrics`, by URL.
//! ClickHouse disk use and Kafka lag come from its own read-only ClickHouse user and Kafka's
//! offsets ([`data_metrics`], ADR 0042), when it's given them.

pub mod data_metrics;
mod freshness;
pub mod kube;
mod pdp_metrics;
mod restarts;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::{bail, Context};
use awzy_heartbeat::{
    is_version, Heartbeat, HeartbeatAnswer, Metrics, Service, ServiceStatus, SupportSessionState, SupportStatus,
    SCHEMA_VERSION,
};
use awzy_model::InstallId;
use axum::{extract::State, routing, Json, Router};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use serde::{Deserialize, Serialize};
use serde_json::json;

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub type HttpClient = awzy_http::Client;

/// HTTP and HTTPS (rustls with the webpki roots).
pub fn http_client() -> HttpClient {
    awzy_http::client()
}

/// Where the reporter learns what's running.
#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    /// Each service's base URL, probed directly. For local runs.
    Probe(Vec<(Service, String)>),
    /// The Kubernetes API, in these namespaces. PDP, gateway, and connector pods are scraped on
    /// `pdp_metrics_port`. `release` is the Argo CD Application installing the release, as
    /// (namespace, name), when the chart lets the reporter read it.
    Kubernetes { namespaces: Vec<String>, pdp_metrics_port: u16, release: Option<(String, String)> },
}

#[derive(Debug, Clone)]
pub struct Config {
    pub install_id: InstallId,
    pub token: String,
    /// The corp portal, e.g. https://corp.awzysecurity.com.
    pub corp_url: String,
    pub source: Source,
    pub chart_version: Option<String>,
    pub interval: Duration,
    pub data: data_metrics::DataConfig,
}

/// `awzy-pdp=http://awzy-pdp:8080,awzy-policy=http://awzy-policy:8080`
pub fn parse_targets(s: &str) -> anyhow::Result<Vec<(Service, String)>> {
    s.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| {
            let (name, url) = t.split_once('=').with_context(|| format!("{t:?}: expected <service>=<url>"))?;
            let service = Service::parse(name)
                .with_context(|| format!("{name:?} is not an awzy service the heartbeat can report"))?;
            Ok((service, url.trim_end_matches('/').to_string()))
        })
        .collect()
}

impl Config {
    /// AWZY_INSTALL_ID, AWZY_REPORTING_TOKEN, AWZY_REPORTER_CORP_PORTAL_URL, optionally AWZY_CHART_VERSION and
    /// AWZY_REPORTER_INTERVAL_SECS (default 60), and the source:
    /// - in Kubernetes: AWZY_REPORTER_NAMESPACES (comma-separated), and optionally
    ///   AWZY_REPORTER_RELEASE_APPLICATION (`<namespace>/<name>` of the Argo CD Application);
    /// - otherwise: AWZY_REPORTER_TARGETS (`<service>=<url>,…`).
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
        let interval = std::env::var("AWZY_REPORTER_INTERVAL_SECS").ok().map(|v| v.parse()).transpose()?.unwrap_or(60);
        if interval < 10 {
            bail!("AWZY_REPORTER_INTERVAL_SECS must be at least 10");
        }
        let source = if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
            Source::Kubernetes {
                namespaces: var("AWZY_REPORTER_NAMESPACES")?
                    .split(',')
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty())
                    .collect(),
                pdp_metrics_port: 8080,
                release: std::env::var("AWZY_REPORTER_RELEASE_APPLICATION")
                    .ok()
                    .and_then(|v| v.split_once('/').map(|(ns, name)| (ns.to_string(), name.to_string()))),
            }
        } else {
            Source::Probe(parse_targets(&var("AWZY_REPORTER_TARGETS")?)?)
        };
        Ok(Self {
            install_id: var("AWZY_INSTALL_ID")?.parse()?,
            token: var("AWZY_REPORTING_TOKEN")?,
            corp_url: var("AWZY_REPORTER_CORP_PORTAL_URL")?.trim_end_matches('/').to_string(),
            source,
            chart_version: std::env::var("AWZY_CHART_VERSION").ok(),
            interval: Duration::from_secs(interval),
            data: data_metrics::DataConfig::from_env()?,
        })
    }
}

async fn get(client: &HttpClient, url: &str) -> Option<(u16, Bytes)> {
    let req = client.request(axum::http::Request::get(url).body(Full::new(Bytes::new())).ok()?);
    let res = tokio::time::timeout(PROBE_TIMEOUT, req).await.ok()?.ok()?;
    let status = res.status().as_u16();
    let body = tokio::time::timeout(PROBE_TIMEOUT, res.into_body().collect()).await.ok()?.ok()?.to_bytes();
    Some((status, body))
}

#[derive(Deserialize)]
struct VersionInfo {
    version: Option<String>,
    image_digest: Option<String>,
}

async fn probe(client: &HttpClient, service: Service, base: &str) -> ServiceStatus {
    let ready = matches!(get(client, &format!("{base}/readyz")).await, Some((200, _)));
    let info = match get(client, &format!("{base}/version")).await {
        Some((200, body)) => serde_json::from_slice::<VersionInfo>(&body).ok(),
        _ => None,
    };
    let (version, image_digest) = info.map(|i| (i.version, i.image_digest)).unwrap_or_default();
    // Probing a URL sees one replica.
    ServiceStatus {
        service,
        version,
        image_digest,
        replicas_desired: 1,
        replicas_ready: u32::from(ready),
        restarts_24h: None,
        ready,
    }
}

/// Collects and sends heartbeats, remembering what it needs between them: the PDPs' last
/// counters and pods' restart history.
pub struct Reporter {
    config: Config,
    client: HttpClient,
    kube: Option<kube::KubeApi>,
    pdp: pdp_metrics::Tracker,
    freshness: freshness::Tracker,
    restarts: restarts::RestartTracker,
    data: data_metrics::DataSources,
    /// awzy-challenge, where support session requests go (ADR 0095); None leaves them unanswered.
    support: Option<ChallengeClient>,
    /// What awzy-challenge last said of each support session, for the next heartbeat.
    support_states: Vec<SupportSessionState>,
}

/// awzy-challenge's client, with this reporter's service token.
pub type ChallengeClient =
    awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient<awzy_service_auth::AuthChannel>;

impl Reporter {
    pub fn new(config: Config) -> anyhow::Result<Self> {
        let kube = match config.source {
            Source::Kubernetes { .. } => Some(kube::KubeApi::in_cluster()?),
            Source::Probe(_) => None,
        };
        Ok(Self::with_kube(config, kube))
    }

    fn with_kube(config: Config, kube: Option<kube::KubeApi>) -> Self {
        let data = data_metrics::DataSources::new(config.data.clone());
        Self {
            config,
            client: http_client(),
            kube,
            pdp: Default::default(),
            freshness: Default::default(),
            restarts: Default::default(),
            data,
            support: None,
            support_states: vec![],
        }
    }

    /// Brings support session requests from the corp portal's answers to awzy-challenge (ADR 0095).
    pub fn with_support(mut self, challenge: ChallengeClient) -> Self {
        self.support = Some(challenge);
        self
    }

    /// Hands the corp portal's support requests to awzy-challenge, and keeps what it says of each
    /// session for the next heartbeat. Without awzy-challenge, requests wait in the corp portal.
    pub async fn relay_support(&mut self, answer: HeartbeatAnswer) {
        use awzy_authz_proto::{
            challenge::v1::{RecordSupportRequestsRequest, SupportRequest, SupportSessionStatus as Wire},
            prost_types::Timestamp,
        };
        let Some(mut challenge) = self.support.clone() else { return };
        let requests = answer
            .support_requests
            .into_iter()
            .map(|r| SupportRequest {
                qid: r.id.to_string(),
                requested_by: r.requested_by,
                areas: r.areas,
                reason: r.reason,
                duration_minutes: r.duration_minutes,
                requested_at: Some(Timestamp { seconds: r.requested_at.timestamp(), nanos: 0 }),
            })
            .collect();
        match challenge.record_support_requests(RecordSupportRequestsRequest { requests }).await {
            Ok(res) => {
                self.support_states = res
                    .into_inner()
                    .sessions
                    .into_iter()
                    .filter_map(|s| {
                        let status = match s.status() {
                            Wire::Requested => SupportStatus::Requested,
                            Wire::Approved => SupportStatus::Approved,
                            Wire::Rejected => SupportStatus::Rejected,
                            Wire::Ended => SupportStatus::Ended,
                            Wire::Expired => SupportStatus::Expired,
                            Wire::Unspecified => return None,
                        };
                        Some(SupportSessionState {
                            id: s.request?.qid.parse().ok()?,
                            status,
                            expires_at: s.expires_at.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, 0)),
                        })
                    })
                    .take(awzy_heartbeat::MAX_SUPPORT_SESSIONS)
                    .collect();
            }
            Err(e) => tracing::warn!(error = %e.message(), "couldn't hand support requests to awzy-challenge"),
        }
    }

    /// One heartbeat, from what's running now. Metrics need two collections to compare, so the
    /// first heartbeat has none.
    pub async fn collect(&mut self) -> Heartbeat {
        let (services, endpoints) = match (&self.config.source, &self.kube) {
            (Source::Kubernetes { namespaces, pdp_metrics_port, .. }, Some(kube)) => {
                from_kubernetes(kube, &mut self.restarts, namespaces, *pdp_metrics_port).await
            }
            (Source::Probe(targets), _) => (self.probe_all(targets).await, probe_metrics_endpoints(targets)),
            (Source::Kubernetes { .. }, None) => (vec![], vec![]),
        };
        let mut samples = HashMap::new();
        let mut scrapes = Vec::new();
        for (service, key, url) in endpoints {
            if let Some((200, body)) = get(&self.client, &url).await {
                let text = String::from_utf8_lossy(&body);
                if service == Service::Pdp {
                    samples.insert(key.clone(), pdp_metrics::parse(&text));
                }
                scrapes.push((key, freshness::parse(&text)));
            }
        }
        let rates = self.pdp.update(samples, std::time::Instant::now());
        let fresh = self.freshness.update(&scrapes);
        let data = self.data.collect().await;
        let release = match (&self.config.source, &self.kube) {
            (Source::Kubernetes { release: Some((ns, name)), .. }, Some(kube)) => match kube.release(ns, name).await {
                Ok(state) => Some(state),
                Err(error) => {
                    tracing::debug!(%error, "couldn't read the release's Application; leaving it out");
                    None
                }
            },
            _ => None,
        };
        Heartbeat {
            schema_version: SCHEMA_VERSION,
            install_id: self.config.install_id,
            sent_at: chrono::Utc::now(),
            chart_version: self.config.chart_version.clone().filter(|v| is_version(v)),
            services,
            metrics: Metrics {
                decisions_per_second: rates.map(|r| r.decisions_per_second),
                pdp_p99_ms: rates.and_then(|r| r.p99_ms),
                error_rate: rates.and_then(|r| r.error_rate),
                kafka_lag: data.kafka_lag,
                clickhouse_disk_used_percent: data.clickhouse_disk_used_percent,
                containment_feed_age_seconds: fresh.containment_feed_age,
                policy_sync_age_seconds: fresh.policy_sync_age,
                principals_sync_age_seconds: fresh.principals_sync_age,
                connector_sync_age_seconds: fresh.connector_sync_age,
                gateway_pdp_unreachable: fresh.gateway_unreachable,
            },
            support_sessions: self.support_states.clone(),
            release,
        }
    }

    async fn probe_all(&self, targets: &[(Service, String)]) -> Vec<ServiceStatus> {
        let probes: Vec<_> = targets
            .iter()
            .cloned()
            .map(|(service, base)| {
                let client = self.client.clone();
                tokio::spawn(async move { probe(&client, service, &base).await })
            })
            .collect();
        let mut services = Vec::with_capacity(probes.len());
        for p in probes {
            services.push(p.await.expect("probe task panicked"));
        }
        services
    }

    /// Sends one heartbeat. The corp portal answers 204, or 200 with support requests.
    pub async fn send(&self, hb: &Heartbeat) -> anyhow::Result<HeartbeatAnswer> {
        let body = serde_json::to_vec(hb)?;
        let req = axum::http::Request::post(format!("{}/v1/heartbeat", self.config.corp_url))
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", self.config.token))
            .body(Full::new(Bytes::from(body)))?;
        let res = tokio::time::timeout(SEND_TIMEOUT, self.client.request(req)).await.context("timed out")??;
        let status = res.status();
        if !status.is_success() {
            let body = res.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
            bail!("the corp portal answered {status}: {}", String::from_utf8_lossy(&body));
        }
        if status == axum::http::StatusCode::NO_CONTENT {
            return Ok(HeartbeatAnswer::default());
        }
        let body = res.into_body().collect().await?.to_bytes();
        serde_json::from_slice(&body).context("the corp portal's answer")
    }
}

/// Whose `/metrics` the heartbeat reads: the PDPs for throughput and latency, and these three for
/// freshness.
const SCRAPED: [Service; 3] = [Service::Pdp, Service::Gateway, Service::Connectors];

/// The awzy workloads in `namespaces`, and the pods to scrape (service, pod UID, metrics URL).
async fn from_kubernetes(
    kube: &kube::KubeApi,
    restart_tracker: &mut restarts::RestartTracker,
    namespaces: &[String],
    port: u16,
) -> (Vec<ServiceStatus>, Vec<(Service, String, String)>) {
    let mut workloads = Vec::new();
    let mut pods = Vec::new();
    for ns in namespaces {
        match (kube.workloads(ns).await, kube.pods(ns).await) {
            (Ok(w), Ok(p)) => {
                workloads.extend(w);
                pods.extend(p);
            }
            (Err(e), _) | (_, Err(e)) => {
                tracing::warn!(namespace = %ns, error = %e, "couldn't read workloads; leaving them out")
            }
        }
    }
    let now = chrono::Utc::now();
    let samples: Vec<_> = pods.iter().map(|p| (p.uid.clone(), p.restarts, p.started_at)).collect();
    let restarts = restart_tracker.update(now, &samples);

    let mut services = Vec::new();
    for w in &workloads {
        // Only services the heartbeat schema knows; anything else stays in the cluster.
        let Some(service) = Service::parse(&w.name) else { continue };
        let its_pods: Vec<_> = pods.iter().filter(|p| p.workload == w.name).collect();
        let version = w.version.clone().or_else(|| its_pods.iter().find_map(|p| p.image_tag.clone()));
        services.push(ServiceStatus {
            service,
            version: version.filter(|v| is_version(v)),
            image_digest: its_pods.iter().find_map(|p| p.image_digest.clone()),
            replicas_desired: w.desired,
            replicas_ready: w.ready,
            restarts_24h: Some(its_pods.iter().map(|p| restarts.get(&p.uid).copied().unwrap_or(0)).sum()),
            ready: w.ready > 0,
        });
    }
    let endpoints = pods
        .iter()
        .filter(|p| p.ready)
        .filter_map(|p| Some((SCRAPED.into_iter().find(|s| s.as_str() == p.workload)?, p)))
        .filter_map(|(s, p)| {
            p.ip.as_ref().map(|ip| (s, p.uid.clone(), format!("http://{}:{port}/metrics", bracket(ip))))
        })
        .collect();
    (services, endpoints)
}

fn probe_metrics_endpoints(targets: &[(Service, String)]) -> Vec<(Service, String, String)> {
    targets
        .iter()
        .filter(|(s, _)| SCRAPED.contains(s))
        .map(|(s, base)| (*s, base.clone(), format!("{base}/metrics")))
        .collect()
}

/// An IPv6 pod IP in a URL needs brackets.
fn bracket(ip: &str) -> String {
    if ip.contains(':') {
        format!("[{ip}]")
    } else {
        ip.to_string()
    }
}

/// The heartbeat the reporter last tried to send, and whether the corp portal took it.
#[derive(Debug, Clone, Serialize)]
pub struct LastSend {
    pub heartbeat: Heartbeat,
    pub delivered: bool,
    pub error: Option<String>,
}

/// Shared between [`run`], which writes it, and [`router`], which serves it.
pub type LastSent = Arc<RwLock<Option<LastSend>>>;

/// Collects, logs, and sends a heartbeat every interval, forever, remembering the last in `last`.
/// A failed send is logged and retried at the next interval.
pub async fn run(mut reporter: Reporter, last: LastSent) {
    let mut tick = tokio::time::interval(reporter.config.interval);
    loop {
        tick.tick().await;
        let hb = reporter.collect().await;
        // The client can read exactly what leaves their install.
        tracing::info!(heartbeat = %serde_json::to_string(&hb).unwrap_or_default(), "sending heartbeat");
        let (delivered, error) = match reporter.send(&hb).await {
            Ok(answer) => {
                if !answer.support_requests.is_empty() {
                    tracing::info!(requests = answer.support_requests.len(), "support session requests from awzy");
                }
                reporter.relay_support(answer).await;
                (true, None)
            }
            Err(e) => {
                tracing::warn!(error = %e, "heartbeat not delivered");
                (false, Some(format!("{e:#}")))
            }
        };
        let sent = LastSend { heartbeat: hb, delivered, error };
        *last.write().unwrap_or_else(|e| e.into_inner()) = Some(sent);
    }
}

/// `GET /v1/last-heartbeat`: where heartbeats go, how often, and the last one exactly as sent
/// (null before the first). For the client portal, inside the cluster only.
pub fn router(config: &Config, last: LastSent) -> Router {
    let about = json!({ "corp_url": config.corp_url, "interval_secs": config.interval.as_secs() });
    Router::new()
        .route(
            "/v1/last-heartbeat",
            routing::get(move |State(last): State<LastSent>| {
                let mut body = about.clone();
                async move {
                    body["last"] = json!(*last.read().unwrap_or_else(|e| e.into_inner()));
                    Json(body)
                }
            }),
        )
        .with_state(last)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use awzy_heartbeat::{ReleaseHealth, ReleaseSync};
    use axum::{
        extract::Query,
        http::HeaderMap,
        routing::{get as get_route, post},
        Json, Router,
    };
    use serde_json::json;

    use super::*;

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn config(source: Source, corp_url: String) -> Config {
        Config {
            install_id: InstallId::new(),
            token: "awzyrt_test".into(),
            corp_url,
            source,
            chart_version: Some("0.1.0".into()),
            interval: Duration::from_secs(60),
            data: Default::default(),
        }
    }

    #[test]
    fn targets_name_only_known_services() {
        let t = parse_targets("awzy-pdp=http://pdp:8080/, awzy-policy=http://policy:8080").unwrap();
        assert_eq!(t, vec![(Service::Pdp, "http://pdp:8080".into()), (Service::Policy, "http://policy:8080".into())]);
        assert!(parse_targets("acme-billing=http://x").is_err());
        assert!(parse_targets("awzy-pdp").is_err());
    }

    /// A PDP whose /metrics counts up by `step` decisions on every scrape.
    async fn fake_pdp(step: u64) -> String {
        let n = Arc::new(Mutex::new(0u64));
        let app = awzy_server::with_health(Router::new().route(
            "/metrics",
            get_route(move || {
                let n = n.clone();
                async move {
                    let mut n = n.lock().unwrap();
                    *n += step;
                    format!(
                        "awzy_pdp_decisions_total{{verdict=\"allow\"}} {n}\nawzy_pdp_check_errors_total 0\n\
                         awzy_pdp_check_duration_seconds_bucket{{le=\"0.001\"}} {n}\nawzy_pdp_check_duration_seconds_bucket{{le=\"+Inf\"}} {n}\n"
                    )
                }
            }),
        ));
        serve(app).await
    }

    #[tokio::test]
    async fn probes_services_and_delivers_a_valid_heartbeat() {
        let pdp = fake_pdp(500).await;
        let not_ready = serve(awzy_server::with_readiness(Router::new(), Arc::new(|| false))).await;
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = received.clone();
        let corp = serve(Router::new().route(
            "/v1/heartbeat",
            post(move |headers: HeaderMap, body: String| {
                let seen = seen.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer awzyrt_test");
                    seen.lock().unwrap().push(body);
                    axum::http::StatusCode::NO_CONTENT
                }
            }),
        ))
        .await;
        let gone = "http://127.0.0.1:1".to_string();
        let source = Source::Probe(vec![(Service::Pdp, pdp), (Service::Policy, not_ready), (Service::Gateway, gone)]);
        let mut reporter = Reporter::with_kube(config(source, corp), None);

        let first = reporter.collect().await;
        first.validate(chrono::Utc::now()).unwrap();
        assert_eq!(first.metrics.decisions_per_second, None, "nothing to compare the first scrape with");
        let by = |hb: &Heartbeat, s: Service| hb.services.iter().find(|x| x.service == s).unwrap().clone();
        assert!(
            by(&first, Service::Pdp).ready
                && by(&first, Service::Pdp).version.as_deref() == Some(env!("CARGO_PKG_VERSION"))
        );
        assert!(!by(&first, Service::Policy).ready && by(&first, Service::Policy).replicas_ready == 0);
        assert!(
            !by(&first, Service::Gateway).ready && by(&first, Service::Gateway).version.is_none(),
            "unreachable counts as not ready"
        );

        let second = reporter.collect().await;
        assert!(second.metrics.decisions_per_second.unwrap() > 0.0, "500 more decisions since the first scrape");
        assert!(second.metrics.pdp_p99_ms.unwrap() <= 1.0);
        assert_eq!(second.metrics.error_rate, Some(0.0));

        reporter.send(&second).await.unwrap();
        // Byte for byte what was sent. (Decoding it back needn't equal `second`: serde_json's
        // default float parsing can be one ulp off.)
        assert_eq!(received.lock().unwrap()[0], serde_json::to_string(&second).unwrap());

        let rejecting = serve(Router::new().route(
            "/v1/heartbeat",
            post(|| async { (axum::http::StatusCode::UNAUTHORIZED, "unknown reporting token") }),
        ))
        .await;
        let refused = Reporter::with_kube(config(Source::Probe(vec![]), rejecting), None);
        let err = refused.send(&second).await.unwrap_err();
        assert!(err.to_string().contains("401"), "{err}");
    }

    /// A Kubernetes API with the awzy system namespace: two PDP pods (one not ready) and
    /// something that isn't awzy's, plus a ClickHouse StatefulSet in the data namespace.
    async fn fake_kube(pdp_ip: &str) -> String {
        let hex = "c".repeat(64);
        let pdp_ip = pdp_ip.to_string();
        let app = Router::new()
            .route(
                "/apis/apps/v1/namespaces/{ns}/deployments",
                get_route(|axum::extract::Path(ns): axum::extract::Path<String>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>| async move {
                    assert_eq!(headers["authorization"], "Bearer test-token");
                    assert_eq!(q["labelSelector"], "app.kubernetes.io/part-of=awzy");
                    let items = if ns == "awzy-system" {
                        json!([
                            { "metadata": { "labels": { "app.kubernetes.io/name": "awzy-pdp", "app.kubernetes.io/version": "abc123" } },
                              "spec": { "replicas": 2 }, "status": { "readyReplicas": 1 } },
                            { "metadata": { "labels": { "app.kubernetes.io/name": "acme-sidecar" } }, "spec": { "replicas": 1 }, "status": { "readyReplicas": 1 } },
                        ])
                    } else {
                        json!([])
                    };
                    Json(json!({ "items": items }))
                }),
            )
            .route(
                "/apis/apps/v1/namespaces/{ns}/statefulsets",
                get_route(|axum::extract::Path(ns): axum::extract::Path<String>| async move {
                    let items = if ns == "awzy-data" { json!([{ "spec": { "replicas": 1 }, "status": { "readyReplicas": 1 } }]) } else { json!([]) };
                    Json(json!({ "items": items }))
                }),
            )
            .route(
                "/api/v1/namespaces/{ns}/pods",
                get_route(move |axum::extract::Path(ns): axum::extract::Path<String>, Query(q): Query<HashMap<String, String>>| {
                    let (hex, pdp_ip) = (hex.clone(), pdp_ip.clone());
                    async move {
                        let started = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
                        let ready = |r: bool| json!([{ "type": "Ready", "status": if r { "True" } else { "False" } }]);
                        let items = match (ns.as_str(), q["labelSelector"].as_str()) {
                            ("awzy-system", "app.kubernetes.io/part-of=awzy") => json!([
                                { "metadata": { "uid": "p1", "labels": { "app.kubernetes.io/name": "awzy-pdp" } },
                                  "status": { "podIP": pdp_ip, "startTime": started, "conditions": ready(true),
                                              "containerStatuses": [{ "restartCount": 2, "image": "reg/awzy-pdp:abc123", "imageID": format!("reg/awzy-pdp@sha256:{hex}") }] } },
                                { "metadata": { "uid": "p2", "labels": { "app.kubernetes.io/name": "awzy-pdp" } },
                                  "status": { "podIP": "127.0.0.1", "startTime": started, "conditions": ready(false),
                                              "containerStatuses": [{ "restartCount": 1, "image": "reg/awzy-pdp:abc123", "imageID": format!("reg/awzy-pdp@sha256:{hex}") }] } },
                            ]),
                            ("awzy-data", "clickhouse.altinity.com/chi") => json!([
                                { "metadata": { "uid": "c1" }, "status": { "startTime": started, "conditions": ready(true),
                                    "containerStatuses": [{ "restartCount": 0, "image": "clickhouse/clickhouse-server:25.8", "imageID": "" }] } },
                            ]),
                            _ => json!([]),
                        };
                        Json(json!({ "items": items }))
                    }
                }),
            );
        let app = app.route(
            "/apis/argoproj.io/v1alpha1/namespaces/awzy-ops/applications/awzy",
            get_route(|| async {
                Json(json!({ "status": { "sync": { "status": "Synced" }, "health": { "status": "Healthy" } } }))
            }),
        );
        serve(app).await
    }

    #[tokio::test]
    async fn reads_workloads_from_kubernetes() {
        let pdp = fake_pdp(100).await;
        let port: u16 = pdp.rsplit(':').next().unwrap().parse().unwrap();
        let kube = kube::KubeApi::for_test(fake_kube("127.0.0.1").await);
        let source = Source::Kubernetes {
            namespaces: vec!["awzy-system".into(), "awzy-data".into()],
            pdp_metrics_port: port,
            release: Some(("awzy-ops".into(), "awzy".into())),
        };
        let mut reporter = Reporter::with_kube(config(source, "http://unused".into()), Some(kube));

        reporter.collect().await;
        let hb = reporter.collect().await;
        hb.validate(chrono::Utc::now()).unwrap();
        let names: Vec<_> = hb.services.iter().map(|s| s.service.as_str()).collect();
        assert_eq!(names, ["awzy-pdp", "clickhouse"], "acme-sidecar isn't awzy's and isn't reported");
        let pdp = &hb.services[0];
        assert_eq!((pdp.replicas_desired, pdp.replicas_ready, pdp.ready), (2, 1, true));
        assert_eq!(pdp.version.as_deref(), Some("abc123"));
        assert_eq!(pdp.image_digest, Some(format!("sha256:{}", "c".repeat(64))));
        assert_eq!(pdp.restarts_24h, Some(3), "pods started an hour ago: all their restarts count");
        let ch = &hb.services[1];
        assert_eq!((ch.version.as_deref(), ch.replicas_ready), (Some("25.8"), 1));
        assert!(hb.metrics.decisions_per_second.unwrap() > 0.0, "the ready PDP pod was scraped");
        let release = hb.release.expect("the Application was read");
        assert_eq!((release.sync, release.health), (ReleaseSync::Synced, ReleaseHealth::Healthy));
    }

    #[tokio::test]
    async fn the_last_heartbeat_is_served_exactly_as_sent() {
        let received = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = received.clone();
        let corp = serve(Router::new().route(
            "/v1/heartbeat",
            post(move |body: String| {
                let seen = seen.clone();
                async move {
                    seen.lock().unwrap().push(body);
                    axum::http::StatusCode::NO_CONTENT
                }
            }),
        ))
        .await;
        let config = config(Source::Probe(vec![]), corp);
        let last = LastSent::default();
        let served = serve(router(&config, last.clone())).await;
        tokio::spawn(run(Reporter::with_kube(config, None), last.clone()));
        while last.read().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let req = axum::http::Request::get(format!("{served}/v1/last-heartbeat")).body(Full::default()).unwrap();
        let res = http_client().request(req).await.unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(body["interval_secs"], 60);
        assert_eq!(body["last"]["delivered"], true);
        let sent: serde_json::Value = serde_json::from_str(&received.lock().unwrap()[0]).unwrap();
        assert_eq!(body["last"]["heartbeat"], sent);
    }
}
