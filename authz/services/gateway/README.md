# awzy-gateway · enforcement

A policy enforcement point (PEP) for traffic the client doesn't want to instrument by hand. Part
of [awzy-authz](../../README.md).

A reverse proxy for HTTP APIs and an MCP proxy that sees agent tool calls and carries service
account, on-behalf-of person, and task annotations. The MCP proxy is the first PEP
([ADR 0019](../../../docs/adr/0019-mcp-proxy-is-the-first-pep.md)).

- Calls awzy-pdp per request: `call` on the tool, and for tools that act on one resource, `read`
  or `write` on that resource too
  ([ADR 0050](../../../docs/adr/0050-mcp-tool-calls-checked-on-the-resource.md))
- For a browser, turns a challenge verdict into a redirect
- For an API caller or agent, returns a structured error with a challenge ID. The challenge goes
  to the person the account acts for, or to its owner when it acts on its own authority (Slack,
  Teams, email), and the caller retries with the ID once it passes, or gets a final deny when it
  expires
- When the PDP can't be reached, replays only recent allows for tools in a fail-open tier
  ([ADR 0030](../../../docs/adr/0030-gateway-outage-fallback.md))

What's built so far: [platform plan](../../../docs/platform-plan.md#built-so-far). Running it locally:
[DEVELOPMENT.md](../../../DEVELOPMENT.md#mcp-gateway-first-enforcement-point).
