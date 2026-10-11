# 0077. PDPs wait for changes instead of polling: long-polls woken by Postgres notifications

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0054](0054-challenges-and-containments-over-grpc.md) (the containment feed, which
  already works this way) and [0044](0044-pdp-loads-resources-from-awzy-context.md) (the resource
  cache)

## Context

A PDP learned of changes by polling: principals every 5 seconds, the active bundle and mode every
10, and resources by letting a 30-second cache expire. Measured, those intervals were exactly
the freshness the plan reports (4.7 s, 9.4 s, 29.9 s), and every PDP replica asked again whether
or not anything had changed. The platform plan's answer was Valkey and an attribute change feed
over Kafka. The containment feed (0054) showed a smaller one: a service follows a Postgres
NOTIFY channel that its writes fire, and pushes on it, with a heartbeat. It applies a
containment in under 200 ms.

## Decision

- **The same mechanism for the rest.** Triggers fire `pg_notify` when the principals snapshot
  moves (`awzy_principals`), a resource or the work linking it changes (`awzy_resources`), and a
  bundle is published or the enforcement mode changes (`awzy_policy`). Each service follows its
  channels with `awzy_store::changes::Changes`, and a request that would answer "unchanged" waits
  on it instead, up to 25 seconds (below common idle timeouts).
- **Long-polls, not new streams**, so the existing calls keep their shape:
  `/v1/principals?known=<v>&wait=25`, and `GetActive` with `wait_seconds` (an additive protobuf
  field). The PDP asks again as soon as it's answered, so a change reaches it as it's committed.
- **Resources by change log.** awzy-context records every resource whose entity may have changed
  (written, deleted, classified, or linked to work whose assignees changed) in
  `resource_changes`, read as a long-poll at `/v1/resources/changes?after=<cursor>&wait=25`. The
  PDP drops each changed resource from its cache, and every cached one with it in its folder
  chain. The log keeps an hour; a PDP further behind is told to drop everything.
- **Polling stays as the fallback.** A PDP that can't reach the service retries on the old
  intervals. While the change log is heard from, a cached resource lives five minutes rather than
  thirty seconds, since a change drops it sooner; when it isn't, the old thirty seconds apply.
- **Valkey still waits** (0054 said so for containments). Nothing here needs a second store, and
  Postgres notifications carry what the PDPs need at this scale.

Rejected: gRPC streams for principals and resources, like the containment feed. A stream needs
its own heartbeat and reconnect logic in every follower; a long-poll is the existing call with one
more parameter, and its timeout is the heartbeat.

## Consequences

- The freshness targets are bounded by notification latency, not by a polling interval. The
  measurements in the platform plan need to be taken again.
- Each service holds one Postgres connection per channel it follows.
- `awzy_pdp_resource_feed_age_seconds` reports the change log's freshness, with an alert.
