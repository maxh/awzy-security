# 0049. A policy dry-run replays recent requests under both bundles, with today's attributes

- Status: Accepted
- Date: 2026-10-09
- Implements part of: the platform plan's Phase 2 ("client portal policy editor with dry-run")

## Context

A draft bundle, whether a person or Awzy Bot (0048) wrote it, is reviewed by reading Cedar.
That's how a policy that quietly denies a whole team, or opens a confidential folder, gets
published. The question a reviewer actually has is "what would this change?", and the decision
log already holds the answer's raw material: every request the PDPs decided, with its subject,
the person it acted for, the action, the resource, and the request context.

What the log doesn't hold is the entity data the PDP saw: people's groups, a resource's
sensitivity and folders, who was assigned. Logging a snapshot with each decision would multiply
the log's size, for a question that's about the policy, not the past.

## Decision

- **Replay distinct recent requests twice:** under the active bundle and under the draft, both
  with **today's** entities, and report where the verdicts differ. Comparing two bundles on the
  same data isolates the policy change. Comparing against the logged verdict would also count
  every attribute change since, such as someone who changed teams, as the draft's doing.
- **Entities come the way the PDP gets them:** people, groups, and service accounts from
  awzy-context's snapshot, resources and their folders and assignees from awzy-context by ID,
  through the same `awzy_pdp` code (`awzy_pdp::replay`). An unregistered service account is
  denied in both, as the PDP would deny it.
- **What's replayed:** decisions from the last 7 days by default (up to 30), grouped into
  distinct requests and weighted by how often each was made, most frequent first, up to 5,000
  distinct requests. The result says if it was cut off.
- **What's reported:** for each change of verdict (allow to deny, deny to allow, allow to
  challenge, …), how many decisions and distinct requests it covers, and the most frequent
  examples with the policies that decided each side.
- It runs in the client portal backend, on request, with `view` on both Policies and Decisions.
  It never touches the PDPs serving traffic.

## Consequences

- Entities an enforcement point sent inline with a request aren't in the log. Requests that
  depended on them replay with awzy-context's data only (or none), so their verdicts can
  differ for reasons that aren't the draft. Since 0044 enforcement points send IDs, so this
  shrinks over time.
- `risk_score` replays as it was logged in the request context; reasoner-written risk
  attributes, once they exist, replay as they are today.
- A dry-run of 5,000 distinct requests costs a ClickHouse aggregation and up to 5,000 Cedar
  evaluations per bundle, plus one awzy-context lookup per distinct resource: seconds, not
  minutes. Larger installs may need sampling.
