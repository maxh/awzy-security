# 0088. The client portal runs only its own pages' queries

- Status: Accepted; amended by [0090](0090-relay-output-is-generated-not-committed.md) (the persisted queries ship with the built frontend)
- Date: 2026-10-11

## Context

Anyone signed in to the client portal can send its GraphQL API any query. The depth and
complexity limits (#120) bound what one query costs, and the PDP decides every field it reads
(0020), but an arbitrary query is still a way to probe the schema and to read in shapes no page
does. The platform plan asks for persisted queries in production from day one. The Relay compiler
already knows every query the frontend can send.

0024 lets a local frontend run against awzy's own staging and demo installs and against the
deployed corp portal. Its queries can be newer than the release it talks to.

## Decision

- **Both frontends send queries by ID.** The Relay compiler persists each operation to
  `persisted_queries.json`, keyed by the SHA-256 of its text, and the frontends send that ID in
  `extensions.persistedQuery`, over HTTP and over the subscriptions' WebSocket alike. Each
  backend embeds its frontend's file and runs the text for the ID (`awzy_portal::PersistedQueries`,
  a schema extension). An unknown ID, as from a page loaded before an upgrade, is refused with a
  message to reload.
- **In a client's install, the client portal runs nothing else**:
  `AWZY_PORTAL_PERSISTED_QUERIES_ONLY=true`, the chart's default (`clientPortal.persistedQueriesOnly`).
  Query text is refused unless it's one of the frontend's own.
- **awzy's staging and demo installs may turn it off**, so 0024's local frontends keep working
  against them. The corp portal resolves IDs but doesn't enforce: it holds no client data, and
  staff point local frontends at its production.
- Run locally, and in tests, both backends also take query text, so GraphiQL works.

## Consequences

- GraphiQL against a client's install can't run anything but the frontend's queries.
- A query added in a pull request reaches the backend in the same release, since the schema-prs
  workflow regenerates `persisted_queries.json` with the Relay artifacts.
- MCP tools don't go through GraphQL, so they're unaffected.
