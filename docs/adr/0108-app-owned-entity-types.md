# 0108. App-owned entity types: the enforcement point is the source of their attributes

- Status: Accepted
- Date: 2026-10-12
- Amends: [0044](0044-pdp-loads-resources-from-awzy-context.md) (where resource attributes come from)
- Builds on: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (rule 2)

## Context

A design partner's app is a multi-tenant SaaS product with hundreds of resource types and many
millions of rows, whose attributes (tenant, owner, plan, region) the app owns and changes all
the time. Syncing them into awzy-context doesn't scale, and under 0044 a `Resource` awzy-context
doesn't know is decided as `unclassified` at the default tier, which is wrong for data awzy was
never meant to hold.

0045 already says an SDK sends attributes "only for entity types awzy-context doesn't hold". What
was missing is a way to declare such types, and a record of whose attributes a decision used.

## Decision

- **A bundle declares app-owned types in its schema** with the annotation `@awzy("app")` on the
  entity type:

  ```cedar
  @awzy("app")
  entity Customer {
    tenant: String,
    owner: String,
  };
  ```

- **awzy-policy checks the declaration when a bundle is published.** Every entity type a bundle
  adds beyond awzy's own (`Group`, `Human`, `ServiceAccount`, `Resource`, `McpServer`, `McpTool`,
  `PortalArea`, `PortalField`) must be marked `@awzy("app")`, and none of awzy's own may be. So
  every type in a bundle has exactly one source of attributes, and it's visible in the bundle.
- **The PDP takes an app-owned type's attributes from the enforcement point** that sent them. It
  never looks one up in awzy-context and never marks it `unclassified`. A `Resource` keeps 0044's
  behaviour. The base policies are written `resource is Resource`, so they don't apply to an
  app-owned type unless a client's policy says so.
- **The trust boundary:** the app is the system of record for its types' attributes, so awzy
  trusts the app's enforcement point for them as it trusts awzy-context for `Resource`. That
  trust stops at the app's types: principals still come from the IdP through awzy-context, and a
  `Resource` sent by an app is still replaced by awzy-context's.
- **Every decision records where its resource's attributes came from:** `awzy-context`,
  `enforcement-point`, or `default` (a `Resource` awzy-context doesn't know). The decision log
  stores it, and the portal's decision page shows it, so an explanation says whom it trusted.

Rejected: a list of app-owned types outside the schema (a bundle field or an install setting).
It's one more thing to keep in step with the schema, and it would sign separately from it.
Rejected: treating every type awzy doesn't know as app-owned. A typo in a type name would then
silently trust the enforcement point.

## Consequences

- A client can model their app's data in their own bundle, and checks on it are decided on what
  the app sends, at the latency of a check with no awzy-context lookup.
- A compromised app can lie about its own types' attributes. It could already decide not to ask;
  this is the same trust, now written down. Its principals and awzy's resources stay out of its
  reach.
- Bundles published before this ADR have no extra types, so they all pass the new check.
