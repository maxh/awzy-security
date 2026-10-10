//! Prometheus metrics at `/metrics` on every service's port (ADR 0066).
//!
//! Every service gets requests by route, method, and status class, with a latency histogram, and
//! `awzy_build_info`. A service adds its own metrics as a [`Source`]. Labels are bounded sets,
//! never an accessor's, resource's, or policy's ID: metrics leave for whatever backend the client
//! chose, and per-entity numbers belong in ClickHouse.

use std::{
    collections::HashMap,
    fmt::Write,
    sync::{
        atomic::{AtomicU64, Ordering::Relaxed},
        Arc, LazyLock, Mutex, RwLock,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{MatchedPath, Request},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

/// Something that adds its own lines to `/metrics`, in the Prometheus text format.
pub trait Source: Send + Sync {
    fn render(&self, out: &mut String);
}

static SOURCES: Mutex<Vec<Arc<dyn Source>>> = Mutex::new(Vec::new());

/// Adds `source` to this process's `/metrics`.
pub fn register(source: Arc<dyn Source>) {
    SOURCES.lock().unwrap_or_else(|e| e.into_inner()).push(source);
}

/// `awzy_ready`: 1 when `/readyz` would answer 200, so an alert sees a service that's up but not
/// serving.
pub struct Readiness(pub super::Readiness);

impl Source for Readiness {
    fn render(&self, out: &mut String) {
        out.push_str("# HELP awzy_ready Whether this process is ready to take traffic.\n# TYPE awzy_ready gauge\n");
        let _ = writeln!(out, "awzy_ready {}", u8::from((self.0)()));
    }
}

/// Upper bounds, in seconds, of the shared request histogram: 1 ms to 10 s.
pub const REQUEST_BUCKETS: [f64; 10] = [0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.5, 1.0, 10.0];

/// A fixed-bucket latency histogram of lock-free counters.
pub struct Histogram {
    bounds: &'static [f64],
    /// Per bucket, plus one for anything slower (+Inf).
    counts: Box<[AtomicU64]>,
    sum_nanos: AtomicU64,
}

impl Histogram {
    pub fn new(bounds: &'static [f64]) -> Self {
        Self { bounds, counts: (0..=bounds.len()).map(|_| AtomicU64::new(0)).collect(), sum_nanos: AtomicU64::new(0) }
    }

    pub fn observe(&self, elapsed: Duration) {
        let secs = elapsed.as_secs_f64();
        let bucket = self.bounds.iter().position(|&b| secs <= b).unwrap_or(self.bounds.len());
        self.counts[bucket].fetch_add(1, Relaxed);
        self.sum_nanos.fetch_add(u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX), Relaxed);
    }

    /// The `_bucket`, `_sum`, and `_count` lines for `name`. `labels` is empty or `k="v",…`.
    pub fn render(&self, out: &mut String, name: &str, labels: &str) {
        let sep = if labels.is_empty() { "" } else { "," };
        let mut cumulative = 0;
        for (i, bound) in self.bounds.iter().enumerate() {
            cumulative += self.counts[i].load(Relaxed);
            let _ = writeln!(out, "{name}_bucket{{{labels}{sep}le=\"{bound}\"}} {cumulative}");
        }
        cumulative += self.counts[self.bounds.len()].load(Relaxed);
        let _ = writeln!(out, "{name}_bucket{{{labels}{sep}le=\"+Inf\"}} {cumulative}");
        let braced = if labels.is_empty() { String::new() } else { format!("{{{labels}}}") };
        let _ = writeln!(out, "{name}_sum{braced} {}", self.sum_nanos.load(Relaxed) as f64 / 1e9);
        let _ = writeln!(out, "{name}_count{braced} {cumulative}");
    }
}

/// Past this many route, method, and status combinations, new ones count as route `other`, so
/// nothing a caller sends can grow the label set without bound.
const MOST_SERIES: usize = 500;

type SeriesKey = (String, &'static str, &'static str);

#[derive(Default)]
struct Requests {
    series: RwLock<HashMap<SeriesKey, Arc<Histogram>>>,
}

impl Requests {
    fn observe(&self, route: String, method: &'static str, status: &'static str, elapsed: Duration) {
        let mut key = (route, method, status);
        if let Some(h) = self.series.read().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return h.observe(elapsed);
        }
        let mut series = self.series.write().unwrap_or_else(|e| e.into_inner());
        if series.len() >= MOST_SERIES && !series.contains_key(&key) {
            key.0 = "other".into();
        }
        series.entry(key).or_insert_with(|| Arc::new(Histogram::new(&REQUEST_BUCKETS))).observe(elapsed);
    }

    fn render(&self, out: &mut String) {
        out.push_str(
            "# HELP awzy_http_request_duration_seconds HTTP and gRPC requests, by route, method, and status class.\n",
        );
        out.push_str("# TYPE awzy_http_request_duration_seconds histogram\n");
        let series = self.series.read().unwrap_or_else(|e| e.into_inner());
        let mut keys: Vec<_> = series.keys().collect();
        keys.sort();
        for key @ (route, method, status) in keys {
            let labels = format!("route=\"{}\",method=\"{method}\",status=\"{status}\"", escape(route));
            series[key].render(out, "awzy_http_request_duration_seconds", &labels);
        }
    }
}

static REQUESTS: LazyLock<Requests> = LazyLock::new(Requests::default);

/// Adds `/metrics` and times every route already on `app`. Call it last, so every route is timed.
pub fn with_metrics(app: Router) -> Router {
    app.route_layer(axum::middleware::from_fn(track)).route("/metrics", get(metrics))
}

async fn track(req: Request, next: Next) -> Response {
    let route = route_label(&req);
    let method = method_label(req.method());
    let started = Instant::now();
    let res = next.run(req).await;
    if let Some(route) = route {
        REQUESTS.observe(route, method, status_label(res.status()), started.elapsed());
    }
    res
}

/// The route a request matched, or None for the health routes, which would only be noise. A gRPC
/// service is mounted under a wildcard, so its method comes from the path; `MOST_SERIES` bounds it.
fn route_label(req: &Request) -> Option<String> {
    let matched = req.extensions().get::<MatchedPath>().map(MatchedPath::as_str).unwrap_or("unmatched");
    if matches!(matched, "/healthz" | "/readyz" | "/version") {
        return None;
    }
    let grpc = req.headers().get(header::CONTENT_TYPE).is_some_and(|v| v.as_bytes().starts_with(b"application/grpc"));
    Some(if grpc { req.uri().path().to_string() } else { matched.to_string() })
}

fn method_label(m: &Method) -> &'static str {
    match *m {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        _ => "other",
    }
}

/// gRPC answers 200 even for an error, which travels in its trailers; services count those
/// themselves.
fn status_label(s: StatusCode) -> &'static str {
    match s.as_u16() {
        100..=199 => "1xx",
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        _ => "5xx",
    }
}

fn escape(v: &str) -> String {
    v.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// Everything this process reports.
pub fn render() -> String {
    let mut out = String::new();
    let release = super::version()["version"].as_str().unwrap_or_default().to_string();
    out.push_str("# HELP awzy_build_info The release this process runs. Always 1.\n# TYPE awzy_build_info gauge\n");
    let _ = writeln!(out, "awzy_build_info{{version=\"{}\"}} 1", escape(&release));
    REQUESTS.render(&mut out);
    for source in SOURCES.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        source.render(&mut out);
    }
    out
}

async fn metrics() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], render())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn a_histogram_counts_cumulatively() {
        static BOUNDS: [f64; 2] = [0.01, 0.1];
        let h = Histogram::new(&BOUNDS);
        h.observe(Duration::from_millis(5));
        h.observe(Duration::from_millis(50));
        h.observe(Duration::from_secs(1));
        let mut out = String::new();
        h.render(&mut out, "x_seconds", "a=\"b\"");
        assert!(out.contains("x_seconds_bucket{a=\"b\",le=\"0.01\"} 1\n"), "{out}");
        assert!(out.contains("x_seconds_bucket{a=\"b\",le=\"0.1\"} 2\n"), "{out}");
        assert!(out.contains("x_seconds_bucket{a=\"b\",le=\"+Inf\"} 3\n"), "{out}");
        assert!(out.contains("x_seconds_count{a=\"b\"} 3\n"), "{out}");
    }

    #[tokio::test]
    async fn every_route_is_timed_by_its_pattern_not_its_ids() {
        let app = with_metrics(Router::new().route("/v1/things/{id}", get(|| async { "ok" })));
        for path in ["/v1/things/alice", "/v1/things/bob", "/nope"] {
            app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        }
        let res = app.oneshot(Request::get("/metrics").body(Body::empty()).unwrap()).await.unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            text.contains(
                "awzy_http_request_duration_seconds_count{route=\"/v1/things/{id}\",method=\"GET\",status=\"2xx\"} 2"
            ),
            "{text}"
        );
        assert!(!text.contains("alice"), "no IDs in labels");
        assert!(text.contains("awzy_build_info{version="), "{text}");
    }

    #[test]
    fn the_label_set_has_a_ceiling() {
        let r = Requests::default();
        for i in 0..MOST_SERIES + 10 {
            r.observe(format!("/grpc.Fake/M{i}"), "POST", "2xx", Duration::ZERO);
        }
        let series = r.series.read().unwrap();
        assert_eq!(series.len(), MOST_SERIES + 1, "the extra ones share route `other`");
        assert!(series.contains_key(&("other".to_string(), "POST", "2xx")));
    }
}
