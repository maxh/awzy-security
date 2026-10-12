# 0109. Enforcement modes for one action or one tool, below the install

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0007](0007-monitor-before-enforce.md) (monitor mode), [0066](0066-the-reasoner-contains-people-automatically-in-its-own-mode.md) (scopes), [0080](0080-signed-enforcement-modes-and-grants-per-policy.md) (signed modes)

## Context

A design partner wants to enforce on their agents' writes while their reads stay in monitor,
and to try a new tool in monitor while the rest of the install enforces. awzy-policy already keeps
a mode per scope, but the only scopes were the install and automated containments, and the PDP
stamped the install's mode on every decision.

Policy sets were the scope the roadmap named first. What the partner asks for is by action and by
tool: their app's typed tool functions map to Cedar actions, and their agents' tools are MCP tools
behind the gateway. Policy sets can still come later as another scope.

## Decision

- **Two scope kinds below the install:** `action:<action>`, for one Cedar action (`action:write`,
  or an app's own `action:view_customer`), and `tool:<server>/<tool>`, for one MCP tool. Each is
  set like the install's: a signed change in awzy-policy (0080), which the portal asks a security
  key for (`switch_enforcement`).
- **A check takes the mode of its most specific switched scope:** its tool's (a `call` on an
  `McpTool`), then its action's, then the install's. A scope switched to monitor holds under an
  enforcing install, and one switched to enforce applies under a monitoring one. A scope never
  switched follows the next one up.
- **The PDP resolves it.** awzy-policy lists every switched action and tool scope
  (`ListEnforcementSettings`), and each PDP verifies them as it verifies the install's mode: a
  signature it can't verify, or a change older than the one in force, keeps the mode in force for
  that scope. The mode on each decision is the resolved one, so awzy-gateway, SDKs, and the
  decision log all agree without resolving it themselves.
- **The Enforcement page lists each scope** with its mode and what enforcing it would change, from
  the decision log's action and resource columns, and switches them.

Rejected: resolving modes in each enforcement point. They'd each need the scopes and the rules,
and could disagree with the decision log. Rejected: a wildcard or class of tools (`tool:drive/*`).
Classes are what actions are for; a tool class can come later as its own scope if one is needed.

## Consequences

- An install can enforce on writes alone, or keep one new tool in monitor, and every enforcement
  point applies it the same way within a second of the switch (scoped changes wake the PDPs'
  long poll like any mode change).
- Once set, a scope stays set: switching it to the install's mode is how it "follows" again.
  Removing a scope outright can come later if the list grows long.
- Every check now looks up one or two scopes before the install's mode: a hash lookup, skipped
  when no scope has been switched.
