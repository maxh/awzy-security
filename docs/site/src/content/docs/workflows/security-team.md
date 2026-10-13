---
title: Security team
description: Rolling awzy out from monitor to enforce, and the review that follows.
---

Your security team runs awzy. The base policies let it see and change everything in the
[client portal](/client-portal/), and it's who approval requests, containments, and
investigations reach.

## Rolling out

1. **Watch first.** Every install starts in [monitor](/concepts/monitor-and-enforce/): awzy decides
   every request and records the verdict, but nothing is blocked. Let it run on real traffic for
   a week or two.
2. **Read the would-denies.** The Enforcement page shows how many requests would have been denied
   or challenged, and lists the latest. Open one on the Decisions page to see which policy
   produced it. Most are one of two things: a policy that's too strict, or data that's labeled or
   shared wrongly.
3. **Fix the policies.** Edit a draft on the Policies page and run a dry run. It replays last week's
   requests under the draft and lists every one whose verdict would change. Publish when the list
   is what you meant.
4. **Fix the data.** A document marked more sensitive than it is, or shared too widely, is fixed in
   its own system. awzy picks the change up through its connectors.
5. **Enforce.** When the would-deny rate is low and the ones left are the ones you'd want blocked,
   switch the install to enforce on the Enforcement page, touching your security key to confirm
   (add one on the Challenges page first). Switch back the same way if something goes wrong;
   decisions keep being recorded either way.

Leave **automated containments** in monitor for longer, and switch them separately, once the
"would contain" records you've reviewed are ones you'd have made yourself. See
[automated containments](/concepts/challenges-and-containment/#automated-containments).

## Day to day

- **Approvals.** On the Challenges page, under *Waiting for approval*, approve or reject requests,
  with a note. You see every request; resource owners see, and approve, the ones for what they
  own. With
  [Slack connected](/guides/optional-features/#slack-notifications), each request reaches your
  team's channel with a link.
- **Containments.** Contain a principal yourself when you see something wrong, and lift a
  containment once you've reviewed it, with what you found. Review awzy-reasoner's
  *Would contain* records: contain the person, or dismiss the record.
- **Investigations.** On the Investigations page, *Needs review* lists what a model found that a
  person has to decide: a containment proposed for a privileged principal, or a benign finding about
  someone the scores flag. Read the evidence and close each one with your conclusion. See
  [Investigations](/concepts/investigations/).
- **Explaining a decision.** When someone asks why they were blocked, look up the decision by its
  ID on the Decisions page. It shows the policies that decided it and their source.
- **Lost security keys.** Reset a person's keys on the Challenges page, after checking who's asking,
  so they can enroll a new one.

## Changing who can do what

The portal's own access is policy too: `view`, `audit`, and `administer` on each portal area.
`audit` sees everything in an area and changes nothing, such as every challenge and containment.
To let auditors read decisions and change nothing, for example:

```cedar
permit (principal in Group::"auditors", action == Action::"view",
        resource == PortalArea::"decisions");
```

To let them see everything in the portal, every challenge and containment included, and still
change nothing, see [external auditors](/workflows/auditors/#getting-read-only-access).

Within an area, a few sensitive fields are checked on their own: a decision's request context, a
person's attributes and risk score, a resource's sharing, and a challenge's justification.
Whoever can view the area sees them unless a policy forbids it, and the portal, and its MCP server,
show a hidden field as hidden. To keep decisions' request context to your privacy team:

```cedar
forbid (principal, action == Action::"view", resource == PortalField::"Decision.context")
unless { principal in Group::"privacy" };
```

## With your own agents

Your team's agents, such as Claude or an internal copilot, can search and explain decisions, read
policies and investigations, and draft policy changes through the portal's
[MCP endpoint](/client-portal/#mcp-for-your-own-agents). The MCP tools can't publish, switch
modes, or contain anyone.

An agent signed in with a person's own token, rather than a token exchange, is that person to
awzy. A token for the MCP endpoint doesn't work on the portal's GraphQL API, which takes tokens
of its own, but within MCP the agent acts with the person's rights.
See [An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).
