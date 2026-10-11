# 2026-10-11: the example install, `small` profile

The first runs inside an install (#94). They cover two of the deployment plan's scenarios, in part.

## Setup

- **Install:** the example client's, `small` profile. Two `t3.large` nodes (2 burstable vCPUs
  each) carry about 30 pods each. The PDP has 2 replicas at a 250m CPU request.
- **Load:** `pdp_loadtest` from the published PDP image, as a pod in `awzy-system` with a service
  token, against the `awzy-pdp` Service. One gRPC connection, so one PDP pod takes the load. Checks
  are decided against awzy-context's directory (the synthetic company), with 99.8% of resource
  lookups from the PDP's cache.
- **Releases:** `c6bcb61` (steady state), `c6bcb61` to `c1ec105` (rollback), `c1ec105` to
  `76bb397` (upgrade).

## Steady-state checks

| Clients | Checks/s | p50 | p99 | p99.9 |
| --- | --- | --- | --- | --- |
| 1 | 403 | 2.11 ms | 7.47 ms | 15.1 ms |
| 4 | 881 | | 17.8 ms | |
| 32 | 1,273 | | 53.4 ms | |

The decision itself averages 0.39 ms (`awzy_pdp_check_duration_seconds`). The rest under load is
queueing on two shared vCPUs: one PDP pod tops out near 1,300 checks/s.

**Pass condition** (p99 under 10 ms at 1,500 checks/s for 30 minutes): **not met** on `small`.
p99 holds under 10 ms only well below one pod's ceiling. Next: spread the load across pods (more
connections or clients), and measure `standard`, whose PDP gets dedicated cores.

## Upgrade and rollback under load

`pdp_loadtest` at 4 clients for the whole rollout, while bootstrap moved the install between
releases and Argo CD rolled every service.

| Rollout | Duration | Checks | Failed | p50 | p95 | p99 | p99.9 | Max |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Rollback `c6bcb61` → `c1ec105` | 480 s | 557,020 | 1 | 2.94 ms | 6.12 ms | 11.38 ms | 51.5 ms | 92.9 ms |
| Upgrade `c1ec105` → `76bb397` | 600 s | 653,318 | 1 | 3.09 ms | 7.18 ms | 14.95 ms | 51.91 ms | 82.43 ms |

The upgrade crossed a release that moved policy signing to a KMS key, with bootstrap and the chart
moved together. The rollback left awzy-policy's newer migration in place under the older release.

**Pass condition** (zero failed checks and tool calls, with a `helm upgrade` and a node drain):
**not met**. Each rollout failed one check in about 600,000; the node drain wasn't run. Next: find
which pod's shutdown the failed check met (the PDP drains on SIGTERM), and add the drain.

## Afternoon: autoscaling, a retry, and where the CPU goes

Later the same day, on release `9775663`: the PDP, the gateway, and the client portal autoscale
on CPU (#190), and enforcement points and `pdp_loadtest` retry a check once when the connection
breaks under it (#191). Still one gRPC connection per run.

| Run | Duration | Checks | Failed | Checks/s | p50 | p95 | p99 | p99.9 | Max |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Rollout restart of the PDP, 8 clients | 150 s | 180,296 | 0 | 1,202 | 5.73 ms | 13.13 ms | 26.42 ms | 59.36 ms | 82.33 ms |
| Steady, 8 clients, CPU sampled | 75 s | 87,615 | 0 | 1,168 | 5.80 ms | 14.04 ms | 28.49 ms | 58.13 ms | 83.67 ms |

- **No failed checks** through a scale-up from 2 to 4 PDP pods and the start of a rollout. The
  morning's rollouts each failed one check in about 600,000, before the retry.
- **The rollout didn't finish under load.** At 4 PDP pods, the rollout's surge pod and a third
  gateway pod didn't fit on two nodes ("Insufficient cpu"). It finished once the load stopped
  and the autoscaler scaled back to 2, after its 5-minute window (#66).
- **One PDP pod did all the work** (1.23 cores), and the autoscaler's extra pods idled at 1–2
  millicores: a Service spreads connections, not requests, and the run had one connection.
  Enforcement points have the same problem. Fixed by #201: PDPs close connections after about a
  minute, and `pdp_loadtest` opens one per client.
- **CPU elsewhere during the run:** ClickHouse ingesting the run's decisions, 0.48 cores; Argo
  CD's application controller, 0.51 cores; the load generator, 0.28 cores, on the same nodes.
  The busier node was at 95%. The nodes were not throttled (`t3` unlimited, credit balances
  healthy).
- **About 1 ms of PDP CPU per check,** against a 0.39 ms decision: gRPC, service-token
  verification, entity assembly, and the decision log make up the rest. Not yet profiled.

## Outage drill

The PDP scaled to 0 for 6.5 minutes, with the install in Enforce and the `internal` tier failing
open (#108). The demo's day of activity ran at 20 calls/s before and during the outage.

- A contained agent's call was refused although the gateway held a fresh allow for it ("this
  caller's access is contained").
- 198 `internal` calls replayed a recent allow; 199 were refused, never allowed or past the
  5-minute cache. All 3 `confidential` and `crown_jewels` calls were refused.
- Each call waited about 8 seconds on the unreachable PDP before the fallback; #195 bounds it to
  about 4.

## What the runs found

Bugs, all fixed: the demo-data Job's accounts and tool tiers (#160, #167), releases that couldn't
publish (#162, #165), a bootstrap that couldn't be planned (#163), and a channel that could move a
chart past its bootstrap (#171, fixed by #175). In the afternoon: a check lost to each rollout
(#191), checks held on one PDP pod (#201), and a slow outage fallback (#195).

Test decisions were deleted from ClickHouse after each set of runs.
