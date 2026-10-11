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

## What the runs found

Bugs, all fixed: the demo-data Job's accounts and tool tiers (#160, #167), releases that couldn't
publish (#162, #165), a bootstrap that couldn't be planned (#163), and a channel that could move a
chart past its bootstrap (#171, fixed by #175).
