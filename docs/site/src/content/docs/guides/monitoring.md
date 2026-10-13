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
resource cache, and dropped decision records, the freshness targets (containment feed, policy and principal syncs),
awzy-gateway's outcomes and PDP outages, and every service's requests.

## Alerts

Alerts labeled `page` mean decisions are wrong, late, or missing now. `ticket` alerts can wait for
working hours. Each alert's `runbook_url` links its section below. For what a failing
service or data store stops doing, and how to bring it back, see
[troubleshooting](/guides/troubleshooting/). Every install starts in `monitor` mode, where decisions are recorded but nothing is
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

A service runs but isn't ready, so it gets no traffic. A PDP stays not ready until it has a signed
policy bundle and the enforcement mode, the principals, and the containment list. It gets them
from awzy-policy, awzy-context, and awzy-challenge, or, when those can't be reached, from a PDP
that's already running. So a new PDP that stays not ready usually means a source is down and no
other PDP is running: check that awzy-policy is ready, and look for signature errors in the PDP's
log.

### AwzyServiceOnOnePod (ticket)

A service that runs two or more pods has had only one ready for 5 minutes. Nothing has failed,
but losing that pod takes the service down, and for the PDP, every enforcement point falls back.
`kubectl -n awzy-system get pods -o wide` shows the missing pods and where they'd go. Pods
`Pending` with "Insufficient cpu" mean the cluster has no room for them, often after a node was
lost or drained: add a node or larger ones. Two PDPs on one node is the same risk, so check
where they run.

### AwzyPdpChecksSlow (page)

The PDP's p99 check time has been over half a second for a minute, close to the time enforcement
points wait before they use their outage fallback (2 s at awzy-gateway). It's almost always the
PDP waiting on a service that takes connections but doesn't answer: awzy-context with its
database down, or one with no ready pods. Check the freshness alerts below and the services
behind them. When they recover, so does this.

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

### AwzyServiceCallRefused (ticket)

One awzy service refused a call from another, which never happens when the install is set up
right. The alert names the route, the reason, and the caller.

- `unlisted`: no access rule names the method, so every caller is refused. It's a bug in that
  release; upgrade, or report it.
- `not_allowed`: the caller isn't in the route's group, usually a chart setting that names who may
  call what. Check that the chart and bootstrap come from the same release.


### AwzyReasonerStepFailing (ticket)

A step of awzy-reasoner's runs has failed every time for an hour. Each failed run is a warning in
awzy-reasoner's log saying why.

- `scoring`: risk scores aren't updated, so the PDPs decide on old ones. Usually ClickHouse or
  awzy-context is unreachable.
- `containments`: nobody is contained automatically. Usually awzy-challenge or awzy-policy is
  unreachable.
- `investigations`: nobody is investigated. Often the model: its service refused the install's
  identity, or the routing names a model it doesn't offer.
### AwzyPdpContainmentFeedStale (page)

A PDP hasn't heard awzy-challenge's containment feed for over 2 seconds. Until it recovers, the
PDP asks awzy-challenge on every check, which counts against the 10 ms budget. If awzy-challenge
can't answer either, the PDP decides on the last list it heard for up to 5 minutes
(`pdp.containmentsMaxStaleSeconds`), so a restart or a database failover doesn't fail checks, and
after that fails them. A containment added meanwhile doesn't reach that PDP. Check that
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
the last known resource, which may be out of date, but only for as long as its sensitivity tier
allows (`pdp.resourcesMaxStaleSeconds`: an hour for public and internal, 15 minutes for
confidential, 5 for crown jewels). After that, and for any resource not cached, checks fail and
enforcement points use their outage fallback. Check awzy-context and Postgres.

### AwzyPdpDroppingDecisions (page)

PDPs dropped decision records before they reached Kafka, so the audit trail and the client
portal are missing those decisions. Decisions themselves weren't affected: a PDP drops a record
rather than make a check wait. The alert's `reason` says where:

- `buffer_full`: Kafka has been too slow or unreachable for long enough to fill the PDP's buffer.
- `produce_failed`: Kafka refused a batch after every retry.

Check Kafka's brokers and the PDP's log, which names the error. Records already dropped can't be
recovered.

### AwzyPdpDroppingPlans (ticket)

The same for the query plans PDPs make for filtered lists and searches. The plans were still
returned to their callers; only their record is missing.

### AwzyGatewayCantReachPdp (page)

awzy-gateway can't reach the PDP, so tool calls follow the outage tiers set in the client portal:
replayed or denied. Check the PDP's pods and their readiness, then the network between the
gateway and the PDP.

### AwzyReleaseNotSynced (ticket)

Argo CD hasn't synced the install's release for 30 minutes. The install keeps running the release
it has, but takes no newer one, fixes included. `kubectl -n awzy-ops get application awzy -o
yaml` says why, under `status.operationState.message` and `status.conditions`:

- **A release waiting for a newer bootstrap** names the awzy commit to apply bootstrap (and the
  cloud layer) from. See [Installing awzy](/guides/install/), "Upgrades".
- **A sync error** (an object the API server refuses, or a hook Job that failed) is awzy's to fix
  in a release; tell awzy support. Once a fixed release is out, a sync still retrying the old one
  can be stopped with `kubectl -n awzy-ops patch application awzy --type merge -p
  '{"status":{"operationState":{"phase":"Terminating"}}}'`, and Argo CD moves to the new one.

### AwzyReleaseUnhealthy (page)

Argo CD reports the release as Degraded or Missing for 15 minutes: a Deployment that can't become
ready, a Job that failed, or an object that's gone. `kubectl -n awzy-ops get application awzy -o
json | jq '.status.resources[] | select(.health.status != "Healthy")'` names them; then look at
those pods' events and logs.

## CPU profiles

When a service uses more CPU than its load explains, a profile shows where. Set `AWZY_PROFILING=on`
on its Deployment, and `GET /debug/profile?seconds=30` on its port 8080 samples it for that long
and answers with the functions that took the most samples. It's off by default: a profile names
the code's functions, and costs CPU while it runs. Turn it off again when you're done.

## What awzy sees, and what you see

Two views start from the same `/metrics`, for different people.

| | awzy's corp portal | Your monitoring |
| --- | --- | --- |
| **Who sees it** | awzy staff | Your team. awzy has no access |
| **Where it lives** | awzy's own account | Your account, beside the install |
| **How the data gets there** | awzy-reporter scrapes the pods, reduces them to a heartbeat, and sends it once a minute: the only thing that leaves your install | Your Prometheus scrapes every pod directly, at full resolution |
| **What's in it** | A fixed summary: decisions per second, PDP p99, error rate, Kafka lag, ClickHouse disk use, the freshness ages, tool calls during PDP outages, each service's version and replicas, and whether the release synced and its health (Argo CD's states, never its messages). Never data about your people, resources, decisions, or policies | Every series, per pod: the dashboard, the alerts, traces, and your cloud's metrics for Postgres, Kafka, and Valkey (CloudWatch on AWS) |
| **What it's for** | Seeing the fleet: which installs are healthy, how a release is rolling out, where support should look | Running your install: alerts that reach your on-call, and finding why something is slow |

- **They agree.** The heartbeat's PDP p99 and the dashboard's come from the same histogram; the
  heartbeat sends one number a minute instead of the series.
- **The heartbeat is deliberately thin.** It tells awzy that something is wrong and roughly
  where, not why. Finding out why takes your monitoring, a support session your admin approves,
  or a diagnostics bundle your admin reviews and sends.
- **You can see what leaves.** The client portal's Status page shows the last heartbeat sent,
  and you can turn reporting off (bootstrap's `reporter = null`). Your own monitoring doesn't
  depend on it.
