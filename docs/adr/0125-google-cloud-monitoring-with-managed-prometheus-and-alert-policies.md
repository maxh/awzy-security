# 0125. Google Cloud monitoring: Managed Prometheus scrapes, Cloud Monitoring alerts

- Status: Accepted
- Date: 2026-10-13

## Context

[0096](0096-aws-monitoring-with-a-managed-scraper-and-portable-dashboards.md) gave AWS installs
monitoring with nothing added to the cluster: Amazon Managed Prometheus's scraper, the
repository's alert rules, and an SNS topic. Google Cloud installs had nothing. The game day
(#353) found the GCP example install's outages paging no one: no PodMonitoring selects awzy's
pods, and Managed Prometheus's rule evaluator and Alertmanager run zero replicas (#401).

GKE runs Google Cloud Managed Service for Prometheus's collectors on every node already. What
it needs is told what to scrape, and something to evaluate the rules and send alerts. There are
two ways to evaluate:

- Managed Prometheus's rule evaluator and Alertmanager, in the cluster, reading `Rules` resources
  and an Alertmanager configuration Secret. Alertmanager sends email only through an SMTP server
  the install provides.
- Cloud Monitoring alert policies with PromQL conditions, over the same data. They're evaluated
  outside the cluster, and send to Cloud Monitoring notification channels (email and others)
  with no SMTP server.

## Decision

- **`deploy/terraform/gcp/monitoring`**, applied after bootstrap:
  - A `PodMonitoring` per awzy service on its `metrics` port (0124), named after the service.
    Managed Prometheus sets `job` to the PodMonitoring's name and won't relabel it, and the rules
    and dashboards group by `job`. Plus one for Argo CD's controller, whose metrics bootstrap
    already opens.
  - **Each rule in `deploy/observability/prometheus/alerts.yaml` becomes a Cloud Monitoring alert
    policy** with the same PromQL, `for`, and labels. `page` rules are `CRITICAL`, the rest
    `WARNING`. The policy links the rule's runbook. The module reads the file, so a rule added to
    the repository reaches the install the next time it's applied.
  - Email notification channels for `alert_emails`. Without any, incidents still open in Cloud
    Monitoring.
- The example client's Google Cloud install applies it as a third stage, `monitoring`, in CI.

Rejected: Managed Prometheus's rule evaluator and Alertmanager. They're two more things in the
cluster for the client's team to keep running, and email needs an SMTP relay. Rejected for now:
a Grafana for Google Cloud. `deploy/terraform/grafana` works with any Grafana, and Managed
Prometheus has a Prometheus-compatible query API to point one at.

## Consequences

- Both clouds evaluate the same rules over the same metrics, and neither adds a component to the
  cluster.
- The rules' annotations are Prometheus templates (`{{ $labels.job }}`), which Cloud Monitoring
  doesn't expand, so a policy links the runbook and leaves the summary out.
- A rule's metric that Managed Prometheus doesn't collect (container metrics from the kubelets)
  never fires on Google Cloud. None of the current rules uses one.
