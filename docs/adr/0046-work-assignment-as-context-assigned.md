# 0046. Work assignment reaches policies as `context.assigned`, per principal

- Status: Accepted
- Date: 2026-10-09
- Implements part of: [0039](0039-synthetic-organization-for-demo-and-tests.md) (demo plan step 4)

## Context

Two of the demo's scenarios turn on work assignment. In the rogue agent, SalesGenie, acting for
a sales rep, reads the strategic plan, which neither of them has any work that needs. In the
curious contractor, a contractor's credentials open documents outside the statement of work. The
platform plan's goals name "work assignment" as something policies reference and that stays
current without manual entry.

awzy-context already holds work items (tickets, accounts, projects, statements of work) with
their assignees and the resources they link (demo plan step 2), and the PDP loads each resource
and its folders from awzy-context by ID (0044). What's missing is the question a policy asks:
is this principal assigned to work that needs this resource?

## Decision

- awzy-context's `GET /v1/resources/{id}/entities` also returns `assignees`: everyone on an
  **open** work item that links the resource or any folder above it. Work on a folder covers
  what's in it; closed work doesn't count.
- The PDP caches the assignees with the resource (30 seconds, 0044) and sets
  **`context.assigned`** for each Cedar check: whether that check's principal is among them.
  A delegated request is checked twice (0016), so the agent and the person it acts for are each
  asked about their own assignment, and both must pass.
- `assigned` is an optional context attribute, set only by the PDP (callers that send it are
  rejected, like `on_behalf_of`). It's absent when awzy doesn't know, meaning no awzy-context or
  not a `Resource`. Policies test `context has assigned && !context.assigned`, so missing data
  never looks like "not assigned". A resource awzy-context doesn't know has no assignees, so
  `assigned` is false there.
- Assignees match principals by ID: a person's `userName` or a service account's registered ID,
  which is what work items carry.
- Bundles published before `assigned` existed carry a schema without it. For those, the PDP
  doesn't set it rather than failing the check.
- No base policy uses it yet. Scenario policies come with challenges (demo plan step 6).

Rejected: assignees as a set attribute on the `Resource` entity. Cedar sets hold one entity
type, so people and service accounts would need two attributes and every policy would have to
check both. A popular folder's set would also be copied into every check that touches it.
Rejected: ownership counting as assignment. Owning a document and being assigned to work on it
are different signals; ownership can become its own attribute.

## Consequences

- An assignment change reaches decisions within 30 seconds of the resource's next load.
- Assignment is only as good as the work items connectors write. A client with no work
  tracking connected gets `assigned = false` everywhere, so assignment policies should be
  scoped, by sensitivity for example, and rolled out in monitor mode.
- One more indexed query on awzy-context per cache miss (`work_items.resources && <chain>`, GIN).
