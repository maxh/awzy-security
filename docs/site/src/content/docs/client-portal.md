---
title: The client portal
description: Your team's web app for awzy, page by page, and the MCP endpoint for your own agents.
---

The client portal is the web app your install serves for your team. Your security team uses it to
see and explain every decision, write and publish policy, switch from monitor to enforce, manage
the directory, review challenges, containments, and investigations. Everyone else uses it to
answer the challenges sent to them. It runs in your cluster, like the rest of awzy, and reads
only your install's data.

## Signing in, and who can do what

People sign in through your IdP (OpenID Connect). There are no awzy passwords.

What someone can see and change is decided by awzy itself: every page and every action is a check
against your policies, as `view` or `administer` on a **portal area**. The areas are
`decisions`, `policies`, `enforcement`, `directory`, `challenges`, and `investigations`. The base
policies let your security team view and administer all of them, and let everyone view
`challenges`, so they can answer their own. You can change that like any other policy, for
example to let auditors view decisions but change nothing.

These checks apply even while the install is in monitor mode: the portal always enforces its own
access. Every change made in the portal is recorded with who made it.

## Pages

### Decisions

Every allow, deny, and challenge the PDPs decided, newest first. Filter by principal, resource,
verdict, and mode, so that, for instance, *monitor* and *deny* together list what enforcing would
have blocked.

Open a decision to see what was asked (the principal, the person it acted for, the action, the
resource), the verdict and mode, and **why**: the policies that produced it, with their source,
from the bundle in force at the time. For a request made by an agent for a person, it shows both
checks. See [How awzy decides](/concepts/how-awzy-decides/).

### Policies

Your [Cedar](/guides/writing-policies/) policy bundles: the active one, drafts, and the ones they
replaced.

- **New draft:** edit the policies and schema. A draft that doesn't validate isn't saved, and the
  errors say why.
- **Dry run:** replays the last week's distinct requests under the active bundle and the draft,
  with today's people and resources, and lists every request whose verdict would change, most
  frequent first. That's what you'd break before you break it.
- **Publish:** signs the draft and makes it active. Every PDP picks it up as soon as it's
  published, usually within a second. If the draft would stop you administering policies, the
  portal says which policy would do it and publishes only if you confirm: afterwards, only a
  break-glass rollback could undo it.
- **Awzy Bot,** when your install turns it on: ask it why something was denied, or to draft a
  policy. It searches and explains decisions and reads the active policies, as you, and only ever
  drafts: a person publishes. It runs on your cloud's own model service.

### Enforcement

Whether awzy enforces its verdicts or only records them. See
[Monitor, then enforce](/concepts/monitor-and-enforce/).

- **Mode:** monitor or enforce for the install, with who switched it and when. Switching takes a
  touch of your security key.
- **What enforcing would change:** over the last 24 hours, how many decisions there were, how
  many would have been denied or challenged, and how many were. The most recent would-denies are
  listed, so you can check them before you switch.
- **Automated containments:** a separate switch for the containments awzy-reasoner makes on its
  own. It starts in monitor, where they're only recorded, as "would contain".
- **History:** every switch, with who made it.

### Directory

Who awzy knows about.

- **People and groups,** as your IdP provisions them over SCIM, with search. These are read-only
  here: your IdP owns them.
- **Service accounts:** register each backend service, scheduled job, or AI agent with its
  accountable owner, whether it's an AI agent, and its groups. One that isn't registered is
  denied everything.

### Challenges

What's waiting for people. See [Challenges and containment](/concepts/challenges-and-containment/).

- **Waiting for you:** challenges sent to you. Give a reason, or touch your security key.
- **Waiting for your approval:** requests you may approve, such as for data you own. Approve or
  reject each, with a note. Those who administer challenges see every waiting request instead.
  See [Data owners and approvers](/workflows/approvers/).
- **Containments:** who is contained, and why. Contain a principal yourself, or lift a
  containment with a reason. While automated containments are in monitor, the ones awzy-reasoner
  would have made appear as "would contain": contain the person, or dismiss the record.
- **Your security keys:** add and remove the keys you use for verification. Your first key needs
  only your sign-in; changing them later needs a key you already have. Admins can reset someone's
  keys when one is lost.

When your install connects Slack, people get a message when something waits for them here, with
a link.

### Investigations

What awzy-reasoner found when it looked into principals it flagged, when your install has
investigations turned on.

Each one shows its verdict, its risk next to what the deterministic score said, the evidence it
cited, what awzy's checks changed in the model's finding, and what became of any containment it
recommended. **Needs review** lists those a person has to decide: a proposed containment of a
privileged principal, or a benign finding about someone the scores flag. Close an investigation
once you've reviewed it, with what you found. It stays, as the record of that review.

## MCP for your own agents

The portal also serves an [MCP](https://modelcontextprotocol.io) endpoint at `/mcp`, so your
security team's own agents, such as Claude or an internal copilot, can work with awzy. Each call
is checked the same way the pages are: with a token-exchange token, as the agent and as the person
it acts for; with the person's own token, as the person.

| Tool | Does |
| --- | --- |
| `search_decisions` | Search decisions by principal, resource, verdict, mode, and time |
| `explain_decision` | One decision, with the source of the policies behind it |
| `get_active_policy_bundle` | The policies and schema the PDPs enforce |
| `list_policy_bundles` | Recent bundles, with their status |
| `check_access` | Would this principal be allowed this action on this resource now? Decided like a check, but nothing is logged and no challenge is opened |
| `dry_run_policy` | Replay recent requests under a draft and list every verdict it would change |
| `propose_policy` | Save a draft, for a person to publish in the portal |
| `get_enforcement` | The mode, and the last 24 hours' would-deny and would-challenge counts |
| `list_investigations` | Investigations, by status |
| `get_investigation` | One investigation, with its evidence and what the checks changed |

All but `propose_policy` only read, and it only drafts. Nothing through MCP publishes policy,
switches modes, or contains anyone.

The MCP endpoint takes tokens for its own audience (`awzy-mcp` unless your install sets another),
and the portal's GraphQL API doesn't accept them, so a token handed to an agent for MCP can't
answer challenges or approve anything. MCP clients find where to sign in from the endpoint's OAuth
metadata. Prefer a token exchange for agents, so they're known as agents. See
[An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).
