# 0005. Managed Postgres, Kafka, and Valkey; ClickHouse in the cluster

- Status: Accepted
- Date: 2026-10-08

## Context

awzy cannot log in to a client install (0009), so every stateful system it runs there must
recover by itself or be fixable by the client's team from a runbook. Each store also has a
distinct job: transactions (Postgres), append-only scans over large volumes (ClickHouse), a feed
between services (Kafka), and hot-path reads (Valkey).

Dropping Kafka in favor of ClickHouse async inserts and a Postgres outbox was proposed to shrink
the footprint. It was rejected: Kafka stays.

## Decision

- Postgres, Kafka, and Valkey default to each cloud's managed service (on AWS: Aurora PostgreSQL,
  MSK, ElastiCache for Valkey). CloudNativePG, Strimzi, and a Valkey chart are fallbacks only for
  clients who refuse managed services.
- ClickHouse runs in the cluster under the Altinity operator with ClickHouse Keeper, because the
  product depends on ClickHouse-specific features (Kafka engine, materialized views, TTL to object storage).
- Losing ClickHouse must lose nothing: decisions never wait on it, and it can be rebuilt from
  Kafka and the object-storage cold tier.

## Consequences

- ClickHouse is the one hard stateful system in the client cluster. Who operates it (the client's
  team with runbooks, or ClickHouse Cloud BYOC) is still open.
- Whether the `small` profile can drop Kafka is still open. The default profile keeps it.
