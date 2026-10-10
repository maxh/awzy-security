# awzy-policy · management

Stores policy bundles with versions and distributes them, signed, to the PDPs. Part of
[awzy-authz](../../README.md).

Runs validation and Cedar schema checks, supports dry-run by replaying past decisions from
ClickHouse, and holds each scope's enforcement mode
([ADR 0007](../../../docs/adr/0007-monitor-before-enforce.md)). The client portal writes policies
through this API. Bundles are signed at publish
([ADR 0021](../../../docs/adr/0021-signed-policy-bundles.md)), and `awzy-break-glass` rolls back
a bad one without the IdP, PDP, or portal
([ADR 0022](../../../docs/adr/0022-break-glass-rollback.md)).

- awzy-policy is the only source of truth for what is enforced
- Clients who want GitOps get a one-way sync: git proposes bundles through the same API, and
  client portal edits are blocked for a policy set that git manages

Binaries: `awzy-policy` (the service), `awzy-publish` (publishes the release's base bundle),
and `awzy-break-glass`.

What's built so far: [platform plan](../../../docs/platform-plan.md#built-so-far). Running it locally:
[DEVELOPMENT.md](../../../DEVELOPMENT.md#policy-bundles).
