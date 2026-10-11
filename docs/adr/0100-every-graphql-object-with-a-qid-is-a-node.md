# 0100. Every GraphQL object with a QID is a Node, and CI checks the schemas

- Status: Accepted
- Date: 2026-10-11
- Amends: [0058](0058-qid-fields-are-named-qid-on-the-wire.md) (how GraphQL objects carry their QID)

## Context

0058 named QID fields `_qid` on the wire and kept `id` on GraphQL objects, which Relay requires.
It didn't say which objects implement Relay's `Node` interface, and only some did: `Challenge`,
`Decision`, and `PolicyBundle` in the client portal and `Install` in the corp portal, while
`Containment`, `Investigation`, `SecurityKey`, and `SupportSession` had an `id: ID!` that
`node(id)` couldn't find. Without `Node`, Relay can't refetch an object by itself, so pages refetch
whole lists after a change instead.

The conventions were also only as consistent as review made them. Some QIDs were typed `String`
(`decisionQid`, `addedWithQid`), and some arguments holding a QID were named `id`.

## Decision

- **An object with `id: ID!` implements `Node`**, and `node(id)` finds it. It routes on the type
  the QID parses as, and it checks the same permission as the query that lists the object.
- **`id: ID!` is only for awzy's own QIDs.** Objects whose ID comes from the client's systems
  (people, groups, service accounts, resources, `EntityRef`, `Actor`) keep `id: String!` and
  aren't Nodes: their IDs aren't unique across types, and awzy didn't mint them.
- **Any other ID-typed field, input field, or argument holds a QID**, so it's named `qid` or
  `<entity>Qid` and is typed `ID`. The exception is `node(id)`, which Relay names.
- **`tools/ci/check-graphql-schemas.mjs` enforces these** on both committed schemas, along with
  `UtcTimestamp` for every field ending in `At`. It runs in `pnpm lint`, so CI's lint-ts job fails
  on a schema that breaks them.
- **Where a service has no lookup by ID**, `node(id)` may search what the service lists (recent
  containments, the viewer's own security keys). That is a stopgap until the service gains a
  lookup by ID.

## Consequences

- Relay can refetch any awzy object on its own, and new types get `Node` because CI requires it.
- The checks run on the generated schema, so a mistake shows up there even though it's made in the
  Rust that generates it.
- `node(id)` for a containment searches only the latest 500, and for a security key finds only the
  viewer's own, until awzy-challenge can look them up by ID.
