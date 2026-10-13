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
@id("auditors-see-the-portal")
permit (
  principal in Group::"auditors",
  action in [Action::"view", Action::"audit"],
  resource is PortalArea
);
```

`view` shows each page as anyone allowed there sees it, and `audit` adds what only those who run
an area otherwise see: every challenge and every containment. Neither lets you change anything:
publishing, switching modes, containing, and approving all need `administer`, or the security key
of whoever does them. The security team can narrow
what you see, such as keeping requests' context to its privacy team, without changing this
policy. See [changing who can do what](/workflows/security-team/#changing-who-can-do-what).

This shows you everything in the table below. On the **Challenges** page you see every
challenge and containment, without the buttons to approve, contain, or lift.

## The evidence

| Control                                             | Where                                 | What it shows                                                                                                                                                                                        |
| --------------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access is decided per action                        | **Decisions**                         | Every allow, deny, and challenge: who, for whom, which action, which resource, when, and in which mode. [Exported](#sampling-and-exporting-decisions) for a period as CSV or JSON                    |
| Decisions follow approved policy                    | **Decisions**, open one               | The policies that produced it, with their source, from the bundle in force at the time                                                                                                               |
| Policy changes are controlled                       | **Policies**                          | Every bundle: drafts, the active one, and those it replaced, each with its version and who wrote it, and who published the active one. A draft is validated and can be dry-run before it's published |
| Emergency changes are controlled                    | **Policies**, Break-glass rollbacks   | Each rollback made with an emergency key enrolled at install: when, the key that signed it, the reason, the version it restored, and the bundle it published. See [break-glass rollback](/guides/break-glass/) |
| Enforcement is on, and changes to it are controlled | **Enforcement**, History              | Every switch between monitor and enforce, for the install and for each action or tool, with who made it and when. Each switch takes a touch of the person's security key                             |
| Sensitive actions get a second look                 | **Challenges**                        | Each challenge, its kind (a justification, a security key, or an approval), who answered it, and their reason or note                                                                                |
| Risky access is stopped                             | **Challenges**, Containments          | Each containment, why, and who or what made it; lifting one takes a reason                                                                                                                           |
| Software has an accountable owner                   | **Directory**, Service accounts       | Each service account and AI agent, its owner (a person from the IdP), whether it's AI, and who registered it                                                                                         |
| People come from the IdP                            | **Directory**, People and groups      | People and groups exactly as the IdP provisions them. They can't be edited in awzy                                                                                                                   |
| Flagged principals are reviewed                     | **Investigations**, when turned on    | Each finding, its evidence, what was done, and the review that closed it                                                                                                                             |
| Vendor access is approved and bounded               | **Support**                           | Each support session awzy staff asked for: the areas, why, who approved it, for how long, and every page they opened                                                                                 |

Who did something is recorded as a person, a service account acting for a person, one of awzy's
own services (such as an automated containment), or a break-glass key: never as free text.

## Sampling and exporting decisions

The **Decisions** page filters by principal, resource, verdict, mode, and a period: **From** and
**To**, in your own time zone. A period can be anywhere in the decision log's history, up to 30
days long, so a quarter is three searches. Without one, the page shows the last 24 hours.

**Export** downloads what the filters select, newest first, as CSV for a spreadsheet or as JSON,
up to 50,000 decisions at a time. Each row holds the decision's ID, time, principal, the person it
acted for and that check's verdict, the action, resource, verdict, mode, the policies that
determined it, and the policy version. If more decisions match than one export holds, the page
says so; narrow the filters or the period to get the rest.

- **Each export is recorded**, with who made it, the period, the filters, and how many decisions it
  held, in the install's activity log beside every other change made in the portal.
- **The request context is included only if you may see it.** If the security team keeps it to
  others, the export leaves it out and says so.
- **The IDs are the ones enforcement points were given**, so a decision in the export can be
  matched to an application's own logs, and opened in the portal with its policies explained.

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
- **A decision made on out-of-date inputs says so.** When a part of the install that a decision
  point reads from was down, the decision point keeps deciding on what it last had, for a limited
  time. Such a decision's "Out of date" field names what was old: the containment list, people
  and groups, the policy, or the resource's attributes.
