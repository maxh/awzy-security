# 0019. The MCP proxy in awzy-gateway is the first PEP

- Status: Accepted; amended by [0050](0050-mcp-tool-calls-checked-on-the-resource.md)
- Date: 2026-10-09
- Amends: 0003 (which left the first PEP open)

## Context

0003 made every policy enforcement point (PEP) an interchangeable caller of one PDP, and left the
first one to whatever the first design partner needs. There is no design partner yet, and Phase 1
can't finish without a PEP that applies verdicts. The candidates were the MCP proxy in
awzy-gateway, a language SDK, and an Envoy `ext_authz` filter.

This chooses before a design partner, which is the sales judgment 0003 warned against mixing into
the architecture. It is acceptable here because 0003 also makes the choice cheap to reverse: the
next PEP calls the same `Authorizer/Check`.

## Decision

Build the MCP proxy in awzy-gateway first:

- `/mcp/{upstream}` in front of a client's MCP servers (streamable HTTP). Every `tools/call` is
  checked as action `call` on `McpTool::"<upstream>/<tool>"`; other MCP messages pass through.
- Identity comes from a validated bearer token (0008). An RFC 8693 `act` claim makes the caller a
  `ServiceAccount` acting for the token's subject (0016).
- In enforce mode a deny or challenge returns an MCP tool error with the decision ID and the call
  never reaches the upstream. In monitor mode it is forwarded (0007).
- The caller's token is never forwarded upstream.

Why this one:

- Agents are where Beyond Zero differs from a policy engine, and the delegation model (0016) only
  shows its value on an agent path.
- It needs no change to client apps or their MCP servers: the client points agents at the gateway.
- The paper's agent scenarios ("rogue agent") run through it, so Phase 4 builds on it directly.

## Consequences

- Until a design partner confirms the path, the MCP proxy may not be what they deploy first. The
  next PEP (likely an SDK for apps the partner writes) reuses the PDP API, the identity rules, and
  the monitor/enforce handling.
- The gateway holds tool sensitivity and the service account registry in its config until
  awzy-context owns them.
- Challenges are returned as blocked until awzy-challenge exists (Phase 3).
