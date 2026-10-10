# deploy/observability

The operator view of an install (ADR 0068): what whoever runs it watches. Every awzy service
serves Prometheus metrics at `/metrics` on its own port; these files are what reads them.

- `grafana/dashboards/`: Grafana dashboards as JSON. `awzy-operator.json` covers readiness,
  releases, the PDP's checks, latency, Cedar time, and resource cache, the freshness targets
  (containment feed, policy and principals syncs), awzy-gateway's outcomes and PDP outages, and
  every service's requests.
- `prometheus/alerts.yaml`: alert rules for the platform plan's latency and freshness targets.

Locally, `docker compose -f deploy/local/compose.yaml --profile observability up -d` runs
Prometheus (http://localhost:9091) and Grafana (http://localhost:3000) with both, scraping the
services you've started with `bazel run`. The AWS monitoring module will ship the same files to
managed Prometheus and Grafana.

Edit a dashboard in Grafana, export it as JSON (Share, Export, without "external"), and save it
here; keep `uid` and the datasource uid `prometheus`.
