# awzy-reasoner · slow path

Consumes decisions and events, scores risk, and opens investigations. Part of
[awzy-authz](../../).

Queries ClickHouse for activity windows and peer baselines, scores risk, and opens
investigations that an LLM agent works through using read-only tools over ClickHouse and the
world model. Writes risk attributes and can request containments.

- Scales with KEDA on Kafka lag
- Every automated containment is recorded with its evidence
- Treats all event content as untrusted input; see
  [Securing awzy itself](../../../docs/plans/platform-plan.md#securing-awzy-itself)

## Built so far: risk scores (ADR 0056)

Every 15 minutes (`AWZY_REASONER_INTERVAL_SECS`) it reads the last 30 days of decisions and
events from ClickHouse, scores each person's and service account's last 24 hours from 0 to 100,
and writes the scores to awzy-context (`POST /v1/risk`). The PDPs pick them up with the
principals snapshot and use each principal's own score as `context.risk_score`.

A score is deterministic ([`score.rs`](src/score.rs)): resources opened against the principal's
own usual day (after five active days of history), against its team's median (the smallest group
it's in, with at least three other active members), and denials. Either comparison alone adds at
most 50, so it takes both, or one plus denials, to pass the base policy's challenge threshold.

ClickHouse access is a read-only `awzy_reasoner` user that can select `decisions` and `events`
and nothing else; `awzy-clickhouse-migrate` creates it from the `awzy-clickhouse-reasoner` Secret.

Investigations (an LLM agent with read-only tools, raising scores but never lowering them below
the deterministic one) come next.
