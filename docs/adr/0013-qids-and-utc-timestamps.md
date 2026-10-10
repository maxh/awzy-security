# 0013. QIDs and `UtcTimestamp`

- Status: Accepted; amended by [0058](0058-qid-fields-are-named-qid-on-the-wire.md) (QID field names)
- Date: 2026-10-08

## Context

IDs from different entities look the same as bare UUIDs, which makes logs and support tickets
hard to read and mix-ups easy. Untyped timestamps invite time zone bugs.

## Decision

- IDs are typed per entity, with one Rust type per entity, and formatted as QIDs:
  `<entity_type>_<uuid>` with a UUIDv7 (`decision_…`, `policy_bundle_…`).
- The QID is used wherever an ID leaves a service. Storage keeps native UUID columns.
- Timestamps are `DateTime<Utc>` in Rust. The GraphQL scalar is `UtcTimestamp` (RFC 3339, UTC,
  milliseconds), not `DateTime`.
- Type names are fully qualified and easy to search for (`ClientPortalSchema`, `CorpPortalQuery`),
  not generic (`Schema`, `Query`).

## Consequences

- UUIDv7 IDs sort by creation time, which suits ClickHouse ordering and cursor pagination.
