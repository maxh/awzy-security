---
title: External auditors
description: "Gathering evidence from awzy for an audit: who could access what, who changed the rules, and the record of both."
---

You're auditing a company that runs awzy, for SOC 2, ISO 27001, SOX, or your own controls. awzy
records every access decision, every change to the rules behind them, and who made each change,
in the company's own cloud account. This page says where each piece of evidence is, and what it
shows.

## Getting read-only access

Ask the security team for a sign-in through the company's IdP, in a group whose policies let you
view the portal and change nothing. The portal's own access is policy, so one policy does it:

```cedar
@id("auditors-view-the-portal")
permit (principal in Group::"auditors", action == Action::"view", resource is PortalArea);
```

Viewing never lets you change anything: publishing, switching modes, containing, and approving
all need `administer`, or the security key of whoever does them. The security team can narrow
what you see, such as keeping requests' context to its privacy team, without changing this
policy. See [changing who can do what](/workflows/security-team/#changing-who-can-do-what).

## The evidence

| Control                                             | Where                              | What it shows                                                                                                                                                                                        |
| --------------------------------------------------- | ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access is decided per action                        | **Decisions**                      | Every allow, deny, and challenge: who, for whom, which action, which resource, when, and in which mode                                                                                               |
| Decisions follow approved policy                    | **Decisions**, open one            | The policies that produced it, with their source, from the bundle in force at the time                                                                                                               |
| Policy changes are controlled                       | **Policies**                       | Every bundle: drafts, the active one, and those it replaced, each with its version and who wrote it, and who published the active one. A draft is validated and can be dry-run before it's published |
| Enforcement is on, and changes to it are controlled | **Enforcement**, History           | Every switch between monitor and enforce, for the install and for each action or tool, with who made it and when. Each switch takes a touch of the person's security key                             |
| Sensitive actions get a second look                 | **Challenges**                     | Each challenge, its kind (a justification, a security key, or an approval), who answered it, and their reason or note                                                                                |
| Risky access is stopped                             | **Challenges**, Containments       | Each containment, why, and who or what made it; lifting one takes a reason                                                                                                                           |
| Software has an accountable owner                   | **Directory**, Service accounts    | Each service account and AI agent, its owner (a person from the IdP), whether it's AI, and who registered it                                                                                         |
| People come from the IdP                            | **Directory**, People and groups   | People and groups exactly as the IdP provisions them. They can't be edited in awzy                                                                                                                   |
| Flagged principals are reviewed                     | **Investigations**, when turned on | Each finding, its evidence, what was done, and the review that closed it                                                                                                                             |
| Vendor access is approved and bounded               | **Support**                        | Each support session awzy staff asked for: the areas, why, who approved it, for how long, and every page they opened                                                                                 |

Who did something is recorded as a person, a service account acting for a person, one of awzy's
own services (such as an automated containment), or a break-glass key: never as free text.

## What the evidence rests on

- **Everything stays in the company's cloud account.** awzy, the vendor, never receives it. See
  the [security model](/reference/security-model/).
- **Policy bundles are signed**, and the decision points accept only signed ones, so the bundle a
  decision names is the one it ran.
- **The decision log is kept as long as the company sets.** Its retention setting decides how far
  back you can look; a decision older than that can't be shown or explained. Confirm the setting
  covers your audit period. See [data retention and backups](/guides/data-retention/).
- **Monitor mode records but doesn't block.** A decision recorded in `monitor` mode shows what
  enforcing would have done, not what happened. Check the Enforcement history for when each part
  of the install was enforcing.

## What isn't in the portal yet

- **Exporting decisions.** The Decisions page filters by principal, resource, verdict, and mode,
  and shows a decision's full detail, but has no download. For a sample, ask the company's
  platform team for a query of the decision log in ClickHouse, which runs in their account, or
  use an agent through the portal's [MCP server](/client-portal/#mcp-for-your-own-agents), which
  can search and explain decisions as you.
- **Break-glass rollbacks.** Each is recorded with the key that signed it, the reason, and the
  version it restored, and posted to the security team's Slack channel, but the portal doesn't
  list them. Ask the security team for the channel's history. See
  [break-glass rollback](/guides/break-glass/).
