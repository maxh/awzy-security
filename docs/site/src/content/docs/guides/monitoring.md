---
title: Monitoring and alerts
description: What awzy's dashboard and alerts watch, and what to do when each alert fires.
---

Every awzy service serves Prometheus metrics at `/metrics`. awzy ships one Grafana dashboard and a
set of alert rules for them, in
[deploy/observability](https://github.com/maxh/awzy-security/tree/main/deploy/observability).
On AWS, the `aws/monitoring` module runs Amazon Managed Prometheus with a scraper for the cluster
and loads the alert rules, and the `grafana` module loads the dashboard into any Grafana. You can
use your own Prometheus and Grafana instead: load the same files.

## Traces

Every service sends OpenTelemetry traces over OTLP (gRPC) to the collector named in bootstrap's
`tracing.otlp_endpoint`, so a slow check opens as one trace: the enforcement point, the PDP, and
the awzy-context and awzy-challenge calls it made. Traces stay in your account. Send them all
(`sample_ratio = 1`, the default) and let the collector choose which to keep. On AWS,
`aws/monitoring` runs a collector that keeps every failed trace, every one slower than 25 ms, and
1% of the rest, and writes them to X-Ray, which its Grafana reads.

## The dashboard

`awzy-operator` shows each service's readiness and release, the PDP's checks, latency, Cedar time,
and resource cache, the freshness targets (containment feed, policy and principal syncs),
awzy-gateway's outcomes and PDP outages, and every service's requests.

## Alerts

Alerts labeled `page` mean decisions are wrong, late, or missing now. `ticket` alerts can wait for
working hours. Every install starts in `monitor` mode, where decisions are recorded but nothing is
blocked, so most pages there affect the audit trail, not access.

The first checks are the same for every alert:

```sh
kubectl -n awzy-ops get application awzy        # Synced and Healthy? A failed sync shows why
kubectl get pods -n awzy-system -n awzy-portal -n awzy-data
kubectl -n awzy-system logs deploy/<service> --since=15m
```

### AwzyServiceDown (page)

A service hasn't answered a metrics scrape for 5 minutes: its pods are gone, crashing, or
unreachable. Look at the pods' status and events (`kubectl -n awzy-system describe pod`), then
their logs. A pod that won't start after an upgrade usually says why in its first log lines.

### AwzyServiceNotReady (page)

A service runs but isn't ready, so it gets no traffic. A PDP stays not ready until it has loaded a
signed policy bundle and the enforcement mode: check that awzy-policy is ready, and look for
signature errors in the PDP's log.

### AwzyPdpCheckLatencyHigh (page)

The PDP's p99 check time has been over 10 ms for 10 minutes. On the dashboard, compare Cedar time
with the total: if Cedar time is low, the PDP is waiting, usually on CPU (more checks than its
pods have cores for) or on a stale feed (the alerts below). Add PDP replicas or CPU, or find the
caller sending the extra checks.

### AwzyPdpCheckErrors (page)

More than 1% of checks have failed for 5 minutes. A failed check is one the PDP couldn't decide,
not a deny; awzy-gateway then follows its outage tiers. The PDP's log names each error.

### AwzyServerErrors (ticket)

A service has answered more than 5% of its HTTP requests with a 5xx for 10 minutes. Its log names
the failing requests.

### AwzyPdpContainmentFeedStale (page)

A PDP hasn't heard awzy-challenge's containment feed for over 2 seconds. Until it recovers, the
PDP asks awzy-challenge on every check, which counts against the 10 ms budget. Check that
awzy-challenge is ready and that the PDP can reach it.

### AwzyPdpPolicySyncStale (page)

A PDP last synced policy over a minute ago, so a newly published bundle or a mode change hasn't
reached it. Check awzy-policy and the PDP's log for bundles it refused, such as a bad signature.

### AwzyPdpPrincipalsSyncStale (page)

A PDP last synced principals over a minute ago, so changes from your IdP aren't in its decisions
yet: a removed person may still be allowed. Check awzy-context and its SCIM provisioning.

### AwzyPdpResourceFeedStale (ticket)

A PDP hasn't heard awzy-context's resource changes for over a minute. Cached resources expire
after 30 seconds until it recovers, so decisions stay current but cost more. Check awzy-context.

### AwzyPdpServingStaleResources (ticket)

PDPs are deciding on cached resources because awzy-context can't be reached. Decisions still use
the last known resource, which may be out of date. Check awzy-context and Postgres.

### AwzyGatewayCantReachPdp (page)

awzy-gateway can't reach the PDP, so tool calls follow the outage tiers set in the client portal:
replayed or denied. Check the PDP's pods and their readiness, then the network between the
gateway and the PDP.
