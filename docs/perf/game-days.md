# Game days

A game day takes one dependency away from an install under load and measures what the PDPs do.
The first was [2026-10-13](2026-10-13-game-day.md) (#353). This page says how to run one, and
what went wrong the first time, so the next run doesn't repeat it.

## Running one

```sh
tools/gameday/gameday.sh <kube-context> <scenario> [away-seconds] [out-dir]
```

The scenarios are `context`, `challenge`, `policy`, `kafka` (in-cluster Strimzi), `kafka-broker`
(one Amazon MSK broker), `postgres`, `node`, and `cold`. The script's header lists what each does
and the `AWZY_TEST_*` variables the cloud ones need (the database instance, the MSK cluster),
which stay in your shell, never in the repository.

The output directory holds:

| File | What it holds |
| --- | --- |
| `events.log` | When each step happened, and the decision records counted at the end |
| `timeline.log` | `pdp_loadtest`'s line a second: verdicts, errors by gRPC code, latency |
| `samples.log` | Every 5 s: each deployment's ready replicas, and each PDP's freshness, drop, and error metrics |
| `records.log` | ClickHouse's decision records by 10 s, against the timeline's checks |

Read the timeline for what enforcement points saw, the samples for why, and the record count for
loss. Each check writes one record, and the load sends 200 warm-up checks first.

The installs serve the public demos, so a run takes part of a demo down for its length. Run one
scenario at a time per install. Two installs can run at once.

## What we learned running the first

- **Turn off Argo CD's automated sync, not just self-heal.** With self-heal off, Argo still
  syncs a new chart version when one is published, and one rolled out in the middle of the
  first run, bringing awzy-policy back 25 s after it was scaled to zero. The harness removes
  `automated` for the run and puts the original back when it exits, even on failure.
- **Wait for rollouts to finish before starting.** A run that began just after Argo resumed
  measured the tail of a rollout too. The harness now waits for every deployment.
- **EKS's API server can't reach pod IPs,** so `kubectl get --raw .../pods/<pod>:8080/proxy/metrics`
  hangs there until it times out. The harness reads metrics through `kubectl port-forward`,
  which goes through the kubelet, on every cloud.
- **Don't follow the load pod's log for the whole run.** `kubectl logs -f` lost its connection
  partway through. The pod keeps its log, so the harness reads it once the pod has finished.
- **The load pod needs the registry's pull secret** (`awzy-registry`), as awzy's own pods have.
- **A pod with no controller stops `kubectl drain`,** and the load pod is one. The drain now
  skips it by label, but on EKS the load pod was still evicted, and that run has no timeline.
  Next time, keep the load pod off the node being drained (#402).
- **Scaling a Service to zero doesn't refuse connections.** On both GKE and EKS a connection to
  a Service with no endpoints hangs until the caller's timeout. So "the service is down" tests
  timeouts, not refused connections, and every timeout on the hot path shows up.
- **Stopping Strimzi's operator to keep a broker down makes the outage longer.** After the
  operator restarted it took 5 minutes to recreate the broker, so a planned 4-minute outage
  lasted 9½, and a PDP can't start meanwhile (#400). Plan for it, or take Kafka away another way.
- **There's no Aurora replica in the example install,** so the `postgres` scenario reboots the
  writer. A failover needs a replica first.
- **Neither install evaluates the alert rules,** and the GCP one doesn't scrape awzy's metrics (#401),
  so which alerts fire is worked out from the sampled metrics against the rules in
  `deploy/observability/prometheus/alerts.yaml`.
- **Use resources awzy-context knows.** `pdp_loadtest` checks three resources it doesn't, which
  get the shortest stale limit, so every check fails 5 minutes into an awzy-context outage.
  Real resources mostly last longer (#402).
- **Leave headroom for the load pod.** It requests 250m CPU, and on the AWS `small` install that
  is part of why evicted pods had nowhere to go.
