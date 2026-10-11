# 0102. Repairs are chart-defined Jobs the client portal starts

- Status: Accepted
- Date: 2026-10-11

## Context

The platform plan's operating model has a client admin fix common ClickHouse trouble from the
client portal, without kubectl: free disk, recover ClickHouse Keeper, restore from a backup. The
portal had no way to act on the cluster. It talks to awzy's services, not to Kubernetes.

The options were: the portal runs the repairs itself, with ClickHouse's admin credentials; a
service of its own that runs them; or the portal starts Kubernetes Jobs. Running them in the
portal puts ClickHouse's admin password in the internet-facing service. A new service is more to
run for something used rarely. A Job runs with the credentials the chart already gives the
migrate Job, and its logs are where a platform team already looks.

## Decision

- **Each repair is a CronJob the chart defines, suspended, so it never runs on a schedule.** It
  runs the ClickHouse migrate image with a `repair <kind>` argument.
- **The portal starts a Job from that CronJob's template**, labelled `awzy.io/repair`. A Role in
  the data namespace lets the portal's service account `get` only the named repair CronJobs and
  `create` and `list` Jobs there. RBAC can't limit which Jobs, so a ValidatingAdmissionPolicy
  admits a Job from the portal's account only if it's one of the repairs as the chart wrote it:
  the migrate image with that repair's arguments, no volumes or init containers, the namespace's
  default account, and no secret but ClickHouse's password. One run of a kind at a time.
- **Only those who may administer the enforcement area start repairs**, as the PDP decides, and
  the start is recorded as the viewer's activity like every portal mutation. Runs show on the
  Status page; their output is the Job's log.
- **The first repair is ClickHouse disk cleanup**: it drops detached parts, empties ClickHouse's
  own logs of queries and metrics, and applies decisions' and events' retention now. None of it
  loses awzy's data.
- **Restore waits for backups (#115), and Keeper recovery for a profile that runs Keeper.** The
  small profile's single ClickHouse replica has none. Each will be another repair kind.

## Consequences

- A client admin can free ClickHouse disk from the portal, and later restore and recover, without
  cluster access.
- The client portal's service account can create Jobs in the data namespace, limited by the
  admission policy to the repairs. A compromised portal could start a repair, which runs with
  ClickHouse's admin password as the migrate Job does, but not change what it does.
- The admission policy needs Kubernetes 1.30 or later, where ValidatingAdmissionPolicy is GA.
- Each new repair adds a CronJob, its name to the Role, and its label to the policy, in the chart.
