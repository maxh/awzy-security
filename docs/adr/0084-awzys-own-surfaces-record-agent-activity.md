# 0084. awzy's own surfaces record what agents do as activity

- Status: Accepted
- Date: 2026-10-11
- Builds on: [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (risk scores) and
  [0081](0081-agents-get-tokens-of-their-own-and-say-which-client-they-are.md) (the MCP client's name)

## Context

awzy-reasoner scores principals from decisions and events. A tool call through awzy-gateway left
a decision but no event, and a call to the client portal's MCP endpoint left neither: the portal
checks its own access with the PDP, but nothing recorded what an agent then did with it. The plan
says awzy governs its own MCP server like any other: checked, and recorded as agent activity.

## Decision

- **A small client, `awzy-activity`,** sends events in awzy-intake's app schema
  (`POST /v1/events`), with the intake token, from a background queue: a batch a second, dropped
  when the queue fills or awzy-intake can't be reached. Recording never slows or fails a call.
- **awzy-gateway** records each tool call it forwards: `call` on `McpTool::"<server>/<tool>"`, by
  the principal and the person it acts for, with `client` and the MCP client's name. A failure is
  marked when the upstream refused it. Blocked calls are in the decision log already.
- **The client portal** records each MCP tool call the PDP allowed, as `call` on
  `McpTool::"awzy-portal/<tool>"`.
- **No argument values** go into events, only what names the principal, the tool, and the
  resource.
- **The chart** gives both the intake URL and token when awzy-intake is on; bootstrap puts the
  token's Secret in the portal's namespace too.

## Consequences

- Agents' use of awzy's own tools counts toward their risk, like any other activity.
- Under load, or with awzy-intake down, some activity is lost rather than queued without bound.
