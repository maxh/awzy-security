# 0012. Portal stack; one deployment per portal

- Status: Accepted
- Date: 2026-10-08

## Context

There are two portals. The client portal runs in each client cluster for client admins, employees,
and their agents. The corp portal runs in the vendor account for awzy staff. Both need a GraphQL
API, a web frontend, and an MCP server.

## Decision

- Backends are Rust (axum + async-graphql) and follow the Relay server specification: `Node` with
  global IDs, cursor connections for every list, one input object per mutation.
- Frontends are Vite, React, TypeScript (strict), Relay, and Mantine.
- Each portal backend serves its built frontend and its MCP server (`/mcp`, built with `rmcp`)
  next to `/graphql`. Each portal is one image and one deployment.
- MCP tools call the same Rust service functions as the GraphQL resolvers, so a tool can't see or
  do more than the matching portal action. Tools that would change enforcement only create drafts
  or requests that a human finishes.
- awzy as an MCP server (the portal's `/mcp`) stays separate from awzy enforcing on other MCP
  servers (awzy-gateway).

## Consequences

- No separate frontend server or image.
- Portal actions, including MCP tool calls, are authorized by awzy-pdp (not built yet).
