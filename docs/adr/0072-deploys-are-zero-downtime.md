# 0072. Deploys are zero-downtime: expand-then-contract migrations, drained shutdowns, and two of everything

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0030](0030-gateway-outage-fallback.md) (the gateway's fallback during a PDP outage)
  and [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (a stable wire contract)

## Context

An install sits in front of every access its client makes, so an upgrade can't be an outage.
Much of what that takes is in place: readiness probes (a PDP isn't ready until it has a signed
bundle and the enforcement mode), graceful shutdown on SIGTERM, a protobuf contract that doesn't
break (0045), a heartbeat that only grows optional fields, and on the vendor side, ECS deploys
that start the new task before stopping the old, with rollback when one fails.

Five gaps remained. Services migrate their Postgres schema when they start, and nothing stopped a
migration from breaking the version still running beside it during a rollout. A pod stopped
accepting connections the moment it got SIGTERM, while Kubernetes was still routing to it for a
few seconds. Nothing kept a node drain from evicting every replica of a service at once, and most
services ran one replica. And the client portal's MCP endpoint kept sessions in memory, so a
rollout or a second replica dropped its clients. And sqlx refuses to start a service whose database has a
migration it doesn't know, which is exactly the old version's situation if one of its pods
restarts mid-rollout, or after a rollback.

## Decision

- **Migrations expand, then contract.** During a rollout the old version runs against the new
  schema, so a migration may only add: tables, nullable columns or columns with a default,
  indexes (created concurrently on large tables). Removing or renaming something takes two
  releases: one that stops using it, then one whose migration drops it. Such a migration says so
  on the statement's line with `-- awzy: contract (stopped using in <version>)`.
  `tools/ci/check-migrations.sh` fails a pull request whose new migrations (Postgres and
  ClickHouse) drop, rename, retype, or add a `NOT NULL` column without a default, unless the line
  is marked. Dropping and recreating ClickHouse's Kafka queue table, which holds no data, is
  marked the same way.
- **An older version starts against a newer schema.** Services migrate with sqlx's
  `ignore_missing`, so a pod of the old version that restarts mid-rollout, or a rollback to it,
  doesn't refuse to start over migrations it doesn't know. Expand-only migrations are what make
  that safe.
- **Shutdowns drain.** On SIGTERM, `awzy_server` fails `/readyz` at once but keeps serving for
  `AWZY_SHUTDOWN_DELAY_SECS` (the chart sets 10), so Kubernetes stops routing to the pod before it
  stops accepting; then it finishes in-flight requests and closes HTTP/2 and gRPC connections with
  GOAWAY. The grace period (30 s) covers both. Locally the delay is 0.
- **Rollouts surge.** Every Deployment rolls with `maxUnavailable: 0` and `maxSurge: 1`: a new pod
  must be ready before an old one goes.
- **Two of everything that serves.** The PDP, awzy-policy, awzy-challenge, awzy-context, and the
  client portal run two replicas by default, spread across nodes and zones where the cluster has
  them, each with a PodDisruptionBudget of one unavailable, so a node drain takes one at a time.
  awzy-reasoner stays at one: it runs on a schedule, and a pause in it delays a score, not an
  access.
- **No session lives in one pod.** The client portal's MCP endpoint is stateless (each request
  stands alone), like its GraphQL API.
- **Wire contracts only grow** between versions that run side by side: protobuf `v1` (0045), the
  heartbeat (optional fields), GraphQL (the frontend ships in the same image as its backend).
- **The proof is an upgrade under load:** on the example install, a load generator runs against
  the PDP and the gateway through `helm upgrade`, and the upgrade passes only with zero failed
  requests. It's built with the AWS install.

Rejected: running migrations in a separate pre-upgrade Job. It orders the schema change before
the new pods, but the old pods still run against it, so it needs the same rule; and it's one more
thing that can fail between the two halves of an upgrade.

## Consequences

- Removing a column takes two releases. That's the cost of never stopping the old version.
- The shutdown delay adds up to ten seconds per pod to a rollout.
- A cluster needs room for one extra pod per Deployment during a rollout, and two nodes (better,
  two zones) for the spread to mean anything. An install that sets one replica to save cost
  keeps seamless upgrades but accepts a moment of unavailability on node drains.
