# 0118. A named work item decides assignment

- Status: Accepted
- Date: 2026-10-12
- Amends: [0046](0046-work-assignment-as-context-assigned.md) (work assignment as
  `context.assigned`)

## Context

The Beyond Zero paper asks that every agent action be attributable to "a specific agent, a
specific controlling user, and a specific task". Under 0046 a check sees `context.assigned`:
whether its principal is on any open work that links the resource. An agent acting for someone
gets the union of everything that person is assigned, and since agents are rarely assignees in a
tracker, the agent's own check is almost never assigned at all (#361).

## Decision

- **A check can name its work item**: `work_item_id` on `awzy_model::Check` and on the protobuf
  `CheckRequest`, in the client's own IDs as connectors write them (`jira:SALES-12`). It comes
  from a validated token's `awzy_work_item_id` claim, which awzy-gateway and the client libraries
  read beside `awzy_human_present`.
- **awzy-context says which open work items link a resource**, each with its assignees, beside
  the assignees it already returns.
- **With a work item named, assignment is that item's alone.** Nobody is assigned unless the item
  is open and links the resource or a folder above it. Then its assignees are; and for a delegated
  request, when the person the agent acts for is on it, the agent's check and the person's are
  both assigned. Without one, 0046 is unchanged.
- **Every decision records the work item it named**, in the decision log's `work_item_id`, and
  the dry-run replays it.

Rejected: requiring the agent itself to be an assignee of the named item. Trackers assign people,
not agents, so it would leave agents unassigned everywhere, as now.

## Consequences

- An agent sent to do one task reaches what that task links, and not the rest of its person's
  work, and the audit trail says which task it claimed.
- Naming an item can make an agent's own check assigned where it otherwise wouldn't be, so the
  PDP trusts the enforcement point for `work_item_id` as it does for `on_behalf_of`: it must come
  from a token the enforcement point validated, never from a caller's say-so.
- A task ID that awzy-context doesn't have, or that's closed, assigns nobody, so a stale or
  invented task can only narrow access.
