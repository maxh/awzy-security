# 0044. The PDP loads resources by ID from awzy-context, with a local cache; Valkey waits

- Status: Accepted; amended by [0119](0119-a-pdp-outlasts-its-control-plane.md) (stale resources have a limit by tier)
- Date: 2026-10-09
- Implements part of: [0039](0039-synthetic-organization-for-demo-and-tests.md) (demo plan step 3)

## Context

Until now the PDP decided on whatever resource attributes the enforcement point sent. An
enforcement point can't know a document's sensitivity, and one that could be wrong or tampered
with shouldn't be trusted with it. awzy-context now holds resources (demo plan step 2), so the
PDP can look them up.

The platform plan ("Entity data for Cedar") has awzy-context write resource entities to Valkey,
and the PDP read them from Valkey through a local LRU cache that the attribute feed invalidates.
Neither the Valkey writer nor the attribute feed exists yet, and the demo profile's 4,000
documents don't need either.

## Decision

- Enforcement points send a resource's type and ID. For a `Resource`, a PDP configured with
  `AWZY_CONTEXT_ADDR` loads the resource and its parent folders from awzy-context's
  `GET /v1/resources/{id}/entities` (one recursive query, at most 32 levels, cycles cut). What
  awzy-context returns replaces anything the enforcement point sent for those entities.
- Each PDP caches results in memory for 30 seconds, and "not known" for 5. That keeps an
  attribute change inside the plan's 60-second freshness target without an invalidation feed.
  Up to 100,000 entries; when full, expired entries go first, then the whole cache.
- **Unknown resources** (not synced yet) keep the enforcement point's entity if it sent one,
  else get the default tier, `internal`. awzy-context marks resources without a label or
  classifier tier the same way. Both carry `unclassified: true`, a new optional attribute in the
  Cedar schema, so policies can challenge them.
- **awzy-context unreachable:** an expired cache entry is used if there is one. Otherwise the
  check fails as unavailable (gRPC `UNAVAILABLE`, HTTP 503), and the enforcement point falls back
  as it would if the PDP were down ([0030](0030-gateway-outage-fallback.md)). Deciding a
  crown-jewel document as `internal` because awzy-context was down would be worse.
- Bundles published before `unclassified` existed carry the old schema. The PDP drops the
  attribute for them instead of failing the check.
- MCP tools (`McpTool`) and portal areas are unchanged: they aren't in awzy-context yet.

Valkey comes later, as a shared tier between the PDPs' caches and awzy-context, when the scale
profile (about a million documents, 1,500 checks per second) shows awzy-context's Postgres can't
hold the miss rate, or when the attribute feed exists to invalidate it.

Rejected: putting resources in the principals snapshot. It's sized by headcount; resources are
sized by data. Rejected: falling back to the default tier when awzy-context is down. It would
silently downgrade every unloaded resource during an outage.

## Consequences

- awzy-context is on the hot path for cache misses. A cold PDP makes one request per new
  resource, with a 1-second timeout, so its p99 depends on awzy-context's until the cache is
  warm. The scale tests measure this.
- A change to a resource takes up to 30 seconds to reach decisions; a change to a parent folder
  up to 30 seconds after its children are next loaded.
- `AppState::check` is now async.
- Sharing edges (who a document is shared with) aren't part of the entity yet. They come with
  work-assignment and sharing attributes (demo plan step 4).
