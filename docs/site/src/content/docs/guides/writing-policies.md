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
| `automation_score` | For a person, how much their recent requests look like software, 0 to 100. Optional. |
| `client` | The calling software, such as `browser`, `cli`, or `mcp`, as the enforcement point saw it. Optional. |
| `mcp_client` | The MCP client's name, such as `claude-code`, from awzy-gateway. Optional, and only a hint. |

Every field, with its type and who sets it: [API conventions](/reference/api-conventions/#request-context).

## Your app's own data

awzy knows your people, groups, and agents from your IdP, and your documents from its connectors.
An app you build may have data of its own that awzy will never hold, such as customers, accounts,
or orders, with attributes only the app knows. Declare those types in your bundle's schema and
mark them `@awzy("app")`:

```cedar
@awzy("app")
entity Customer {
  tenant: String,
  plan: String,
};

action view_customer appliesTo {
  principal: [Human, ServiceAccount],
  resource: [Customer],
  context: RequestContext,
};
```

```cedar
@id("support-views-their-tenant")
permit (principal in Group::"support", action == Action::"view_customer", resource is Customer)
when { resource.tenant == "acme" };
```

For a type marked this way, awzy decides on the attributes your app sends with the check. It
doesn't look the resource up, and doesn't treat it as unclassified. For awzy's own types it
never trusts what's sent: people and agents come from your IdP, and a `Resource` from awzy's
connectors.

- Every type you add has to be marked `@awzy("app")`, or publishing fails. That way a typo in a
  type's name can't quietly trust the app.
- awzy's own types (`Human`, `Group`, `ServiceAccount`, `Resource`, `McpServer`, `McpTool`,
  `PortalArea`, `PortalField`) can't be marked.
- Give your type a `sensitivity` attribute (`public`, `internal`, `confidential`, or
  `crown_jewels`) and the base policies that read a tier apply to it too, such as agents acting
  on their own never touching crown jewels.
- Each decision says where its resource's attributes came from: the client portal's decision page
  shows it as **Its attributes from**.

## Publishing

Write a draft in the client portal's Policies page (or ask Awzy Bot for one), run a dry run to
see what would change, then publish. Every PDP picks up the new bundle as soon as it's published,
usually within a second. Publishing warns you first if the draft would stop you administering
policies. If a bad bundle locks your admins out anyway, an enrolled admin can roll back with a
break-glass key, which works even when the IdP or the portal is the problem. See
[Break-glass rollback](/guides/break-glass/). Every use is posted
to your security team's Slack channel, with the key and the reason, when Slack is connected.
