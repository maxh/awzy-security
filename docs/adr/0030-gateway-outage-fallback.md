# 0030. What the gateway does when the PDP is unreachable

- Status: Accepted
- Date: 2026-10-09
- Implements part of: [0003](0003-one-pdp-for-every-access-path.md) (every PEP needs a fallback)

## Context

awzy-gateway asks awzy-pdp about every MCP tool call (0019). Until now, if the PDP couldn't be
reached, the gateway allowed everything when the install was last seen in monitor mode and denied
everything otherwise. Denying everything turns a PDP outage into an outage of every agent's tools,
including harmless ones. Allowing everything would make knocking out the PDP a way around policy.

The plan's answer is a signed fallback bundle, compiled from awzy-context at each policy publish,
that maps what a PEP can see to a sensitivity tier, with fail-open or fail-closed set per tier by
the client. awzy-context doesn't exist yet. The gateway does already know a sensitivity for each
tool it lists in its config.

## Decision

While the PDP can't be reached:

- **Monitor mode:** the gateway keeps forwarding, as before. Monitor means a PEP must not block.
- **Enforce mode, or a PDP never heard from:** a call goes through only if all of these hold:
  - the tool is **listed by name** in the gateway's config, with its own tier. A tool covered only
    by the server's default tier fails closed: anything not explicitly classified fails closed;
  - its tier is in `outage.fail_open_tiers`, which the client sets. `confidential` and
    `crown_jewels` can never fail open; the gateway refuses to start if the config says otherwise;
  - the PDP **allowed the same call** (same subject, same person it acted for, same tool) within
    `outage.cached_allow_ttl_secs` (default 5 minutes, at most an hour). Fail-open replays a
    recent allow; it never grants something new.
- Every other call gets an MCP tool error saying the PDP couldn't be reached and why the call
  failed closed. Each replay and each denial is logged.
- The default is no fail-open tiers: an install that sets nothing fails closed for everything.

## Consequences

- An outage shorter than the TTL is invisible to agents repeating work the PDP already allowed on
  low-sensitivity tools, and stops everything else.
- The TTL bounds the exposure to a decision that changed during the outage. When containments
  exist (Phase 3), issuing one must also clear that accessor's cached allows in every gateway, as
  the plan says; until then the TTL is the only limit.
- The cache is per gateway replica and in memory: a gateway restarted during an outage fails closed.
- Tiers and the fail-open settings live in the gateway's config for now, which the client controls.
  When awzy-context classifies tools, they move into the signed policy bundle and the client portal,
  and this record is amended.
