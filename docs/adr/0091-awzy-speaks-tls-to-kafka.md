# 0091. awzy speaks TLS to Kafka

- Status: Accepted
- Date: 2026-10-11
- Builds on: [0006](0006-decision-log-through-kafka-into-clickhouse.md) (decisions through Kafka
  into ClickHouse) and [0015](0015-cloud-agnostic-install.md) (the install contract)

## Context

Every decision and event crosses the install's VPC on its way to Kafka: who did what to which
resource, and the verdict. MSK accepted plaintext from awzy's services, because the client the
PDP, awzy-intake, awzy-reporter, and awzy-clickhouse-migrate share (rskafka) was built without
TLS. A client's security review would find that first.

MSK serves TLS on its own port (9094), so turning TLS on changes the broker addresses too. The
Kafka engine tables that ClickHouse reads decisions and events through have the broker list
written into them by the migration that created them, so they'd keep reading the old port.

## Decision

- **One way to reach Kafka: `awzy-kafka`** (`authz/libs/kafka`). `Brokers::from_env` reads the
  broker list and `AWZY_KAFKA_TLS`; with TLS, brokers are trusted by the Mozilla roots (Amazon's
  included, for MSK) plus any PEM certificates in `AWZY_KAFKA_TLS_CA_FILE`, for a private CA.
  rskafka's `transport-tls` feature is on, with rustls on ring like the rest of awzy.
- **The install contract says whether Kafka is TLS.** `kafka.tls` (default false) sits next to
  `bootstrap_brokers`; the AWS module hands out MSK's TLS brokers with `tls = true`, and bootstrap
  passes both to the chart as `kafka.brokers` and `kafka.tls`.
- **ClickHouse speaks TLS too.** With `kafka.tls`, the chart gives ClickHouse a `<kafka>` config
  (`security_protocol` ssl, the image's CA bundle) that every Kafka engine table uses.
- **Queue tables follow the brokers.** On every sync, awzy-clickhouse-migrate compares each queue
  table's `kafka_broker_list` with the configured brokers, and when they differ recreates the table
  and its materialized view with the new list. The consumer group keeps its offsets, as when
  earlier migrations recreated the queues, so nothing is read twice or skipped.
- **Plaintext closes in a second step.** MSK keeps accepting both (`TLS_PLAINTEXT`) until installs
  have moved to TLS, so the move has no gap: old pods keep using plaintext while new ones roll
  out on TLS. Then `client_broker` becomes `TLS`.
- **The local stack stays plaintext**: `AWZY_KAFKA_TLS` is unset there.

## Consequences

- Decisions and events are encrypted between awzy's pods and the brokers.
- The brokers are still authenticated only by the network (security groups); SASL or mutual TLS
  for awzy's clients is a separate decision.
- Changing brokers recreates the queue tables, which pauses ingestion into ClickHouse for the few
  seconds that takes. Kafka holds the records meanwhile.
- Bootstrap hands out the TLS brokers, so an install's chart must be a release with `kafka.tls`:
  an older one would speak plaintext to the TLS port. Update the chart version with or before
  bootstrap.
