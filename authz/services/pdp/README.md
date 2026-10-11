# awzy-pdp · hot path

The policy decision point: every enforcement point asks it whether one principal may take one
action on one resource, and it answers allow, deny, or challenge. Part of
[awzy-authz](../../README.md).

Embeds `cedar-policy` for static policy. Before evaluating policy it checks the active
containment list, which it holds in memory and keeps current from a push feed. Principal
entities live in an in-process snapshot. Resource entities are loaded by ID from awzy-context
into a local cache ([ADR 0044](../../../docs/adr/0044-pdp-loads-resources-from-awzy-context.md));
Valkey joins as a shared tier when scale requires it (see
[Entity data for Cedar](../../../docs/platform-plan.md#entity-data-for-cedar)). With each resource
comes who is assigned to work on it, which each check sees as `context.assigned`
([ADR 0046](../../../docs/adr/0046-work-assignment-as-context-assigned.md)). Serves
gRPC, an AuthZEN-style HTTP API, and Envoy `ext_authz` once Envoy is used as an enforcement point.

- Stateless and horizontally scaled
- Emits every decision to Kafka
- Target p99 under 10 ms
- Snapshot size is a tracked metric with an alert

What's built so far: [platform plan](../../../docs/platform-plan.md#built-so-far). Running it locally:
[DEVELOPMENT.md](../../../DEVELOPMENT.md#pdp).
