# 0085. An automation score for people whose requests look like software

- Status: Accepted
- Date: 2026-10-11
- Builds on: [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (risk scores),
  [0081](0081-agents-get-tokens-of-their-own-and-say-which-client-they-are.md) (`client` and
  `mcp_client`), and [0084](0084-awzys-own-surfaces-record-agent-activity.md) (activity)

## Context

A coding agent or assistant signed in with a person's own token is that person to awzy. Policies
can see `context.client == "mcp"` on a call through awzy-gateway, but nothing tells them that a
person's day as a whole looks like software's: hundreds of calls through MCP, in bursts no person
clicks through. The plan named the answer: an automation score from awzy-reasoner.

## Decision

- **awzy-reasoner scores each person** 0 to 100 every run, from their last 24 hours of
  decisions and events, beside their risk score: up to 60 points for the share of requests that
  came through MCP (`client = "mcp"`), and up to 40 for requests in their busiest minute beyond 20.
  Fewer than 10 requests in a day are too few to score. Service accounts aren't scored: they're
  software already.
- **It travels like risk:** written with the risk score to awzy-context (`automation`,
  `automation_reasons`), carried in the principals snapshot as `automation`, and seen by each check
  as `context.automation_score` for its own principal. A caller can't set it; a PDP drops it for a
  bundle whose schema doesn't declare it.
- **A base policy uses it:** a person scored above 80 needs their security key to read, write, or
  call anything `confidential` or `crown_jewels` (`automated-people-verify`). The portal's own
  areas aren't covered, so nobody is locked out of the portal by it.

Rejected: lowering the risk score's weight for MCP traffic, or folding automation into risk. They
answer different questions: risk is "is this unusual for them", automation is "is this them".

## Consequences

- A person who really does drive their work through an agent touches their key for sensitive
  data. That is the point; a client who disagrees changes the policy.
- The score is only as good as `client`: requests from enforcement points that don't set it count
  as a person's, so the bursts signal carries the rest.
