---
title: Domain model
description: "The two kinds of thing awzy works with: entities from your systems, and the records awzy creates."
---

awzy works with two kinds of thing, and names them so they can't be confused:

- **Entities from your systems:** people, groups, service accounts, documents, MCP tools. awzy
  didn't create them, so it keeps the IDs your IdP and apps give them. They're written as a type
  and an ID, `Human::"alice"`, and fields holding their IDs end in `_id` (`human_id`).
- **awzy's own records:** decisions, policy bundles, challenges, containments. awzy creates them
  and gives each a [QID](/reference/api-conventions/#qids). Fields holding one end in `_qid` (`decision_qid`).

The rest of this page describes each. How their IDs, times, and context are written on the wire is
in [API conventions](/reference/api-conventions/).

## Entities from your systems

These are the Cedar entity types your policies see. awzy-context keeps them up to date from your
IdP (over SCIM) and from connectors to your apps. Types your own app adds to the policy schema, such as customers or
invoices, are entities from your systems too; their attributes come from your app with each check.
See [writing policies](/guides/writing-policies/#your-apps-own-data).

| Entity                 | What it is                                                                                                                                                                              | Attributes policies can use                                                                                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Human`                | A person, by your IdP's ID for them                                                                                                                                                     | `active`; `title`, `employee_type`, `department`, `division`, `organization`, `cost_center`, `manager`, and `team` when known; member of `Group`s                         |
| `ServiceAccount`       | Anything non-human with its own credentials: a backend service, a scheduled job, an AI agent. Registered in awzy-context; one that isn't is denied before any policy runs               | `owner` (the accountable `Human`), `ai`                                                                                                                                      |
| `Group`                | A group or team from your IdP. Groups can nest                                                                                                                                          |                                                                                                                                                                              |
| `Resource`             | Something people access: a document, a folder, a project. Resources nest                                                                                                                | `sensitivity`, `unclassified`, `owner`, and sharing: `shared_humans`, `shared_groups` (can read), `writer_humans`, `writer_groups` (can also change), `shared_with_everyone` |
| `McpServer`, `McpTool` | An MCP server behind awzy-gateway, and its tools (`<server>/<tool>`)                                                                                                                    | `sensitivity`                                                                                                                                                                |
| `PortalArea`           | An area of the client portal: `decisions`, `policies`, `enforcement`, `directory`, `challenges`, `investigations`                                                                       | `sensitivity`                                                                                                                                                                |
| `PortalField`          | A sensitive field the portal shows, inside its area: `Decision.context`, `PrincipalDetail.attributes`, `PrincipalDetail.riskScore`, `ResourceDetail.sharing`, `Challenge.justification` | `sensitivity`                                                                                                                                                                |

`Human` and `ServiceAccount` are the **principals**: who every check is about. See
[People, agents, and delegation](/concepts/principals-and-delegation/). A resource's sharing
attributes include what it inherits from the folders above it, and every attribute marked
optional in the schema may be missing, so test it with `has` first. Every field a check's context
carries is listed in [API conventions](/reference/api-conventions/#request-context).

**Actions** are `read` and `write` on a `Resource`, `call` on an `McpTool`, `view` and
`administer` on a `PortalArea`, `view` on a `PortalField`, `switch_enforcement` for switching an enforcement mode (asked
besides `administer` on `enforcement`, so policy can challenge the switch itself), and `approve`
for approving someone else's challenged request, by a person or an agent you name in a policy.

### Sensitivity

Every `Resource`, `McpTool`, and `PortalArea` has a `sensitivity`: `public`, `internal`,
`confidential`, or `crown_jewels`. For a resource, two things can set it:

- **The connector**, from the label it has in your system ("Highly Confidential", say).
- **awzy's classifier**, in the background, in order of trust: a label whose text names a tier,
  then markings in the content ("Strictly confidential"), then names and folders ("board",
  "payroll"). Names never make anything `public`; that takes a label or a marking.

Policies see the **higher** of the two, so a label is never lowered. A resource neither has
placed gets your default tier and `unclassified: true`, so policies can challenge it. A resource
awzy-context doesn't know yet is treated the same way.

### Risk

awzy-reasoner scores each person and service account from 0 to 100 every 15 minutes, from their
recent activity against their own and their team's usual, and stores the score with its reasons.
A check sees its own principal's score as `context.risk_score`. See
[Risk scores](/concepts/risk/).

### Work items

A **work item** is open work in your systems: a ticket, an account, a project, a statement of
work. Connectors copy them into awzy-context with the ID and source they have there (a Jira key,
say), each with its assignees (people and service accounts, by their IDs) and the resources the
work needs.

Work items aren't Cedar entities, so policies never see one directly. They reach policies as one
fact per check, `context.assigned`: whether this check's principal is assigned to open work that
links the resource or a folder above it. It's absent when awzy doesn't know, so test it with
`context has assigned`:

```cedar
forbid (principal is ServiceAccount, action == Action::"read", resource)
when { context has assigned && !context.assigned };
```

## Decisions

A decision's **verdict** is `allow`, `deny`, or `challenge`. A challenge lists one or more
**kinds**, and every one must be passed:

| Kind            | What it asks                       |
| --------------- | ---------------------------------- |
| `justification` | The person says why                |
| `verification`  | The person uses their security key |
| `approval`      | Someone policy allows approves it  |
| `biometric`     | Reserved; not issued yet           |

Passing a challenge creates a **grant**: the exact request (same principal, action, and resource)
is allowed for 15 minutes, so the caller can retry it. A rejected challenge denies the request
until it expires. A decision also records its **mode**: in `monitor` the verdict is recorded but
the request is allowed. See [Monitor, then enforce](/concepts/monitor-and-enforce/).

## awzy's own records

| Record             | QID prefix       | What it is                                                                                                                                                            |
| ------------------ | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Decision           | `decision_`      | One verdict on one check, with the policies that produced it. See [How awzy decides](/concepts/how-awzy-decides/)                                                     |
| Policy bundle      | `policy_bundle_` | A signed, versioned set of your Cedar policies: a `draft`, the one `active` bundle, or `superseded` by a later one. See [Writing policies](/guides/writing-policies/) |
| Challenge          | `challenge_`     | What a principal must pass before retrying: a justification, a security key, or an approval                                                                           |
| Containment        | `containment_`   | A principal whose access is revoked until someone lifts it. See [Challenges and containment](/concepts/challenges-and-containment/)                                   |
| Security key       | `security_key_`  | A person's registered WebAuthn key, used to answer verification challenges                                                                                            |
| Event              | `event_`         | One thing a principal did, as an app or cloud audit log recorded it                                                                                                   |
| Enforcement change |                  | A switch between monitor and enforce: which mode, who switched it, and when. Kept as an append-only history                                                           |
| Break-glass event  |                  | A rollback signed with an admin's break-glass key, outside the portal and your IdP: which key, why, and which bundle version it restored                              |
| Risk score         |                  | One per principal, replaced on each scoring run, with the reasons behind it                                                                                           |
| Plan               | `plan_`          | A query plan: which records of a type a principal may list or search, as a filter for your own query. See the [decision API](/reference/decision-api/#query-plans)    |
| Investigation      | `investigation_` | A model's look into one principal the reasoner flagged: what it found, the evidence it cited, and what awzy's checks let it do. Off until you turn investigations on  |

All of them stay in your cloud account.

## Who did something

Wherever awzy records who did something (published a bundle, lifted a containment, approved a
request), it records an **actor**: an entity, and the person it acted for, if any. The portal
shows it as `alice`, or `sales-genie for alice`. An actor is one of:

| Type             | Who                                                                                                                             |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `Human`          | A person, by your IdP's ID                                                                                                      |
| `ServiceAccount` | One of your service accounts, usually acting for a person                                                                       |
| `AwzyService`    | One of awzy's own components, such as `awzy-reasoner` containing a risky principal, or `awzy-bot` drafting a policy for someone |
| `BreakGlassKey`  | A break-glass key, by its fingerprint. Who held it isn't known                                                                  |
| `App`            | An app you trust to sign its own agents' tokens, registering or retiring them                                                   |
