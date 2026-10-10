# awzy-reasoner · slow path

Consumes decisions and events, scores risk, and opens investigations. Part of
[awzy-authz](../../README.md).

Queries ClickHouse for activity windows and peer baselines, scores risk, and opens
investigations that an LLM agent works through using read-only tools over ClickHouse and the
world model. Writes risk attributes and can request containments.

- Scales with KEDA on Kafka lag
- Every automated containment is recorded with its evidence
- Treats all event content as untrusted input; see
  [Securing awzy itself](../../../docs/platform-plan.md#securing-awzy-itself)

A skeleton so far ([platform plan](../../../docs/platform-plan.md#built-so-far)).
