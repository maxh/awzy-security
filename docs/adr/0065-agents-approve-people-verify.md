# 0065. Agents may approve; only people verify; every chain ends at a person

- Status: Accepted
- Date: 2026-10-10
- Amends: [0016](0016-human-and-serviceaccount-principals.md) (who answers challenges, and
  delegation beyond one hop) and [0054](0054-challenges-and-containments-over-grpc.md) (who may
  approve)

## Context

0016 split principals into `Human` and `ServiceAccount`, and 0054 has every challenge answered by
a person: the human asking, the person an agent acts for, or an unattended account's owner, with
approvals from a person policy allows. That assumes a person is near every request. More and more
requests come from agents working for other agents, and a client running many of them will want
an overseer agent to approve routine requests rather than page a person for each.

A challenge is worth something only if whoever made the request can't satisfy it. An agent that
can be steered by prompt injection could also steer another agent answering for it, so letting
agents answer challenges can't mean letting them answer every kind.

## Decision

- **Verification is people only.** It proves a person is present with a security key (0059), so
  no agent can pass it, by definition. Unchanged.
- **Approval may come from a service account**, such as an overseer agent, where a client policy
  names it. The `approve` action's principal is `Human` or `ServiceAccount`. The base policies
  still let only people approve (`security-team-approves` is `principal is Human in
  Group::"security"`), so no agent approves until a client writes a policy for it.
- **Agents approve on their own authority.** An approver acting for someone else is refused, and
  an approval is never from the request's own subject, the person it was for, or whoever answers
  for it (awzy-challenge's existing rule, now comparing entities, not just people's IDs).
- **An agent never approves for its owner.** awzy-challenge passes `approving_for` (the person who
  answers for the challenged request) in the `approve` check's context, and the base policy
  `agents-never-approve-for-their-owner` forbids a service account whose `owner` is that person,
  or any service account when `approving_for` is absent. Otherwise bob's overseer approving bob's
  agent would be bob approving himself.
- **Crown jewels need a person.** `crown-jewels-need-a-person-to-approve` forbids a service
  account approving a `crown_jewels` resource, whatever a client's policies say.
- **Justification stays with the person** the request is for. An agent could write one, but a
  reason is cheap for a model to produce; its value is the audit trail and awzy-reasoner's
  judgment of it, and that needs someone accountable behind it.
- **Every chain ends at a person.** Every service account has an `owner` (0016), and that stays
  required: an unattended chain's challenges go to the top account's owner, and wait rather
  than pass when no one answers.
- **Delegation chains are for `v2`.** `on_behalf_of` holds one person. A subagent acting for an
  orchestrator acting for alice needs a chain (RFC 8693 nests `act` claims): each link checked,
  the strictest verdict winning, with a depth limit, and challenges going to the nearest link
  allowed to answer each kind. That changes the wire contract, so it waits for `v2` (0045); this
  record is its design input.

Rejected: a third principal type for agents. Whose credential a request carries (`Human` or
`ServiceAccount`) and whether software is driving it are different questions; the second is
per request, like delegation, and belongs in the request's context.

## Consequences

- A client turns on an overseer with two policies: `view` on `PortalArea::"challenges"` and
  `approve` for that service account. It then approves through the client portal's
  `decideChallenge` with its own token, and the decision records it as
  `ServiceAccount::<id>` (0060).
- `approving_for` is set by awzy-challenge, the only caller of `approve` checks. A caller that
  left it out would only make agent approvals fail.
- Deciding approvers in Cedar means a client's policy mistake can let an agent approve more than
  intended. The two base `forbid`s bound the worst of it: never for its owner, never crown jewels.
