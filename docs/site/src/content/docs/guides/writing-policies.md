---
title: Writing policies
description: Cedar policies for awzy, from permits to challenges.
---

awzy's policies are [Cedar](https://www.cedarpolicy.com). A bundle is a schema and a set of
policies, versioned in awzy-policy and signed when published. PDPs refuse unsigned or altered
bundles, and any older than the one they've loaded.

## Permit, then forbid

Nothing is allowed until a `permit` matches; any matching `forbid` overrides every permit.

```cedar
@id("engineering-read-internal")
permit (principal in Group::"engineering", action == Action::"read", resource)
when { resource.sensitivity == "public" || resource.sensitivity == "internal" };

@id("deactivated-humans")
forbid (principal is Human, action, resource)
when { principal has active && !principal.active };
```

Give every policy an `@id`: decisions name the policies that produced them by it.

## Challenges

A `forbid` with `@challenge` becomes a challenge instead of a deny. Passing it lifts that policy
for the request.

```cedar
@id("high-risk-verify")
@challenge("verification")
forbid (principal, action, resource)
when { context.risk_score > 80 };

@id("absent-human-approval")
@challenge("approval")
forbid (principal is ServiceAccount, action, resource)
when { context has on_behalf_of && !context.human_present && context.risk_score > 50 };
```

## What's in the context

| Field | What |
| --- | --- |
| `risk_score` | The principal's risk, 0 to 100, from awzy-reasoner. |
| `on_behalf_of` | The person an agent acts for, when it acts for one. |
| `human_present` | Whether someone is there to answer a challenge. |
| `assigned` | Whether the principal has open work on this resource. |
| `client` | The calling software, such as `browser`, `cli`, or `mcp`, as the enforcement point saw it. Optional. |
| `mcp_client` | The MCP client's name, such as `claude-code`, from awzy-gateway. Optional, and only a hint. |

Every field, with its type and who sets it: [Domain model](/reference/domain-model/#request-context).

## Publishing

Write a draft in the client portal's Policies page (or ask Awzy Bot for one), run a dry run to
see what would change, then publish. Every PDP picks up the new bundle as soon as it's published,
usually within a second. Publishing warns you first if the draft would stop you administering
policies. If a bad bundle locks your admins out anyway, an enrolled admin can roll back with a
break-glass key, which works even when the IdP or the portal is the problem.
