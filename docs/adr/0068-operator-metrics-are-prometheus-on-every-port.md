# 0068. Operator metrics are Prometheus text on every service's port; dashboards and alerts live in the repo

- Status: Accepted
- Date: 2026-10-10
- Amends: the platform plan's "OpenTelemetry SDK in every service" (for metrics; traces keep it)

## Context

Whoever runs an install (the client's platform team, or awzy's own engineers on the example
install) can't see how it's doing. awzy-pdp is the only service with metrics: hand-written
Prometheus text at `/metrics`, which awzy-reporter scrapes for the heartbeat (0026). The latency
and freshness targets in the platform plan were measured once, on a laptop, and nothing watches
them since. `libs/telemetry` promises OpenTelemetry export but only sets up logging.

The plan said every service would use the OpenTelemetry SDK and push to a collector. For metrics,
that makes a collector a dependency of seeing any number at all, adds a large dependency tree to
every binary, and replaces what the PDP and the reporter already do.

## Decision

- **Every service serves Prometheus text at `/metrics`** on its existing port, added by
  `libs/server` the same way it adds `/healthz` and `/readyz`. Anything can scrape it: Prometheus,
  managed Prometheus, an OpenTelemetry collector's Prometheus receiver, Datadog's agent.
- **Every service gets the same baseline for free:** HTTP and gRPC requests by route and status
  class, with a latency histogram, and `awzy_build_info` with the release. Services add their own
  metrics as sources (`awzy_server::metrics::register`); awzy-pdp's existing ones become one, under
  the same names, so the reporter doesn't change.
- **Hand-written, no new dependency**, like the PDP's: atomic counters and fixed-bucket
  histograms. The PDP's own decision counters stay lock-free; the shared request series take a
  read lock to find theirs.
- **Names** are `awzy_<service>_<what>_<unit>` (`awzy_pdp_check_duration_seconds`), or `awzy_http_…`
  for the shared baseline.
- **No client data in labels, ever.** Labels are bounded sets (route, verdict, status class),
  never accessor, resource, or policy IDs: metrics leave the install's data stores for whatever
  backend the client chose, and per-entity numbers are ClickHouse's job (the client portal).
  Routes are labelled by their pattern (`/v1/resources/{id}`), requests that match no route
  aren't counted, and past 500 series new ones share the route `other`, so nothing a caller sends
  can grow the label set without bound.
- **`/metrics` needs no token.** It holds counts and latencies only, and the reporter and scrapers
  in the cluster read it; the namespace's network policy limits who reaches it.
- **Dashboards and alerts are files in the repo** (`deploy/observability/`): Grafana dashboards as
  JSON and Prometheus alert rules as YAML. The local stack runs Prometheus and Grafana with them,
  and the AWS monitoring module will ship the same files to managed Prometheus and Grafana.
- **Alerts encode the platform plan's targets:** PDP check p99 under 10 ms, and errors and
  readiness. Freshness targets get alerts once the services that bound them export the numbers.
- **Traces stay on OpenTelemetry**, exported over OTLP and configured by the standard `OTEL_*`
  variables, as `RUST_LOG` is. Not built yet.

Rejected: the OpenTelemetry metrics SDK, for the reasons above. Rejected for now: the
`prometheus-client` crate. The text format awzy needs is a few dozen lines, and the PDP's
existing metrics already use it.

## Consequences

- One dashboard covers every service the day it's deployed, with no per-service work.
- A service's own metrics are written by hand. If that grows past counters and histograms, revisit
  the library choice.
- The baseline histogram's buckets are shared (1 ms to 10 s, plus the PDP's finer ones for its
  own checks), so a service with a different latency profile adds its own histogram.
