# 0006. Decision log through Kafka into ClickHouse

- Status: Accepted
- Date: 2026-10-08

## Context

Every decision must be recorded for explain-decision, decision search, the reasoner's activity
windows, and the audit trail. Writing to ClickHouse from the PDP would put an analytics store on
the hot path.

## Decision

- awzy-pdp writes every decision to the Kafka topic `awzy.decisions.v1`, partitioned by subject,
  using `rskafka` (pure Rust, no librdkafka).
- ClickHouse ingests through a Kafka-engine table and a materialized view. There is no custom
  ingest service.
- Tables live in `authz/clickhouse/` and are applied by `awzy-clickhouse-migrate`, run as a
  Kubernetes Job on each release.
- The `decisions` and `events` topics keep at least 7 days, so ClickHouse can replay after an outage
  or a rebuild.

## Consequences

- A ClickHouse outage pauses search and the reasoner but not enforcement.
- Kafka retention is part of the install's durability, and the Terraform module has to set it.
