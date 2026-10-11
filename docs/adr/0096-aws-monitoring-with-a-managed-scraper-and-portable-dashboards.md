# 0096. AWS monitoring: a managed scraper, and dashboards for any Grafana

- Status: Accepted
- Date: 2026-10-11

## Context

Every awzy service serves Prometheus metrics, and `deploy/observability` has alert rules and a
dashboard (0068), but nothing installed them: a client saw nothing unless they wired their own
monitoring. The deployment plan's step 5 calls for an `aws/monitoring` module. Each component
added to a client's cluster is one more thing their team keeps running (platform plan,
"Operating model").

## Decision

- **`deploy/terraform/aws/monitoring`** creates Amazon Managed Service for Prometheus, its
  **managed scraper**, the repository's alert rules, an alert manager that sends to an SNS topic,
  and an Amazon Managed Grafana workspace signed in to through IAM Identity Center. The scraper
  runs outside the cluster, so no collector or agent is added to it; it finds pods through the
  Kubernetes API with a read-only ClusterRole and scrapes awzy's pods and the kubelets' container
  metrics from the cluster's subnets.
- **`deploy/terraform/grafana`** loads the data sources and every dashboard in
  `deploy/observability` into any Grafana. It's apart from the AWS module because a Grafana
  provider can't be configured from a workspace created in the same apply, and so that a client
  with their own Grafana uses it as is.
- **Cardinality stays low.** awzy's metrics carry no per-principal or per-resource labels, and the
  scraper keeps only the container CPU and memory series the dashboards use.
- `aws/install` outputs where the scraper reaches the cluster (`monitoring`); the example client
  applies both modules as their own stages.

## Consequences

- An install's alerts and dashboards are the repository's own, updated by applying again.
- Managed Grafana needs IAM Identity Center in the install's account.
- The Grafana service account token lasts thirty days; applying `aws/monitoring` again makes a new
  one before `grafana` is applied.
- Clients who use their own monitoring scrape `/metrics` and load `deploy/observability`
  themselves.
