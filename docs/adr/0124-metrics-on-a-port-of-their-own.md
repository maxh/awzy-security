# 0124. Metrics on a port of their own, which any scraper may reach

- Status: Accepted
- Date: 2026-10-13
- Amends: [0068](0068-operator-metrics-are-prometheus-on-every-port.md) (metrics on each service's
  own port, behind the namespace's network policy)

## Context

0068 serves `/metrics` on each service's API port, and leaves who reaches it to the namespace's
network policy. The chart's policies deny ingress by default and admit awzy's own namespaces to
each service. That suits awzy's services calling each other, and awzy-reporter reading the PDP's
metrics. It shuts out a scraper that isn't a pod in one of those namespaces.

Amazon Managed Prometheus's scraper (0096) is one. It connects from the cluster's subnets. In
the AWS example install it has never scraped awzy-challenge, awzy-context, awzy-policy, or six
other services, so `AwzyServiceDown` has fired for them since monitoring was installed, and their
metrics never reached a dashboard (#406). Only the PDP, the gateway, and awzy-intake were scraped,
because their policies admit anyone. Google Managed Prometheus's collectors run in their own
namespace and are shut out the same way (#401). Argo CD had the same problem, and bootstrap opens
its controller's metrics port alone.

Admitting the scraper to each API port would open every service's API to the subnets, guarded
only by service tokens (0055).

## Decision

- **Every service also serves `/metrics`, and nothing else, on a `metrics` port (9090)**, from
  `libs/server` when `AWZY_METRICS_ADDR` is set. The chart sets it in every service, so local runs,
  which start many services on one host, don't contend for the port.
- **A network policy in each awzy namespace admits anything to the `metrics` port**, and only that
  port. Any scraper works with no per-cloud settings: a managed scraper's subnets, GMP's
  namespace, or a client's own Prometheus.
- **The API port still serves `/metrics`**, for awzy-reporter's heartbeat and the local stack's
  Prometheus. Nothing changes for them.
- Scrapers select the port by name (`metrics`). AWS's scrape configuration does.

Rejected: a chart setting listing the scrapers' address ranges. It would need per-cloud values
that change with the network, and still open the API ports to them. Rejected: scraping through
the Kubernetes API's pod proxy. EKS's API server can't reach pod IPs.

## Consequences

- AMP scrapes every awzy service, and `AwzyServiceDown` means what it says.
- Anything that reaches a pod's IP can read its metrics. They hold counts and latencies with
  bounded labels, never client data (0068), and the API port is no more open than before.
- Each service listens on one more port.
