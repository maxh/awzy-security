# 0113. Query plans: a filter over the attributes the app holds, from Cedar's partial evaluation

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0016](0016-human-and-serviceaccount-principals.md) (delegation), [0044](0044-pdp-loads-resources-from-awzy-context.md) (awzy's attributes), [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (the stable proto), [0046](0046-work-assignment-as-context-assigned.md) (`context.assigned`), [0108](0108-app-owned-entity-types.md) (app-owned types)

## Context

An app that enforces authorization in its own data layer needs to ask which resources of a type a
principal may act on, and turn the answer into a query filter. The PDP answers one resource at a
time, so an app filtering a list or a search fetches more rows than it returns and checks each.
The design partner's step 4 is row-level read limits for agents on list and search tools: "this
agent sees only customers A, B and C", beyond what its person may see.

A filter runs in the app's database, over attributes the app's rows hold. Since 0044 the PDP
doesn't trust resource attributes an enforcement point sends for awzy's own types; it loads them
from awzy-context. So who owns the attributes a filter runs over has to be settled first.

## Decision

- **`POST /v1/access/plan`, and `Authorizer/Plan` in the proto**, take an evaluation's principal,
  person, action, and context, and a resource *type* with no ID. The PDP evaluates the policies
  with the resource unknown (Cedar's partial evaluation, with its type known) and answers one of:
  `always_allowed`, `always_denied`, `challenge` (a challenge that depends only on context fires for
  every row; its kinds come with the filter that applies once it's passed), or `conditional` with a
  filter. It always returns the real filter with the mode: the enforcement point applies monitor.
- **The filter is a small documented tree**: `and`, `or`, `not`, comparisons of an attribute path
  with a literal, `has`, set `contains`, `contains_any`, `contains_all`, `like`, and `in` for
  hierarchy membership (`resource in Folder::"x"`, an ancestor-closure join). A principal's own
  groups are expanded where a policy asks whether the principal is in a set the resource holds.
  Anything else comes back as an `unmappable` node with its Cedar text, and anything that depends
  on a per-row fact awzy computes as an `unknown` node; the app checks just those rows one by one.
- **Delegation is the AND of both principals' filters**, as a check is the stricter of two (0016).
  A challenge that depends on the resource counts as a forbid in the filter, and the plan says a
  challenge could widen what's shown.
- **Who owns the attributes:**
  - *App-owned types* (0108): the filter covers attributes the app holds and sends for its own
    entities, such as a tenant or an owner. This is what plans are for.
  - *awzy's types* (`Resource`): a filter over `sensitivity`, folders, or sharing works only if the
    app stores those labels on its rows. The plan can't know whether its copy is current, so each
    plan names the attribute paths it uses, and a plan over awzy's types is for apps that sync
    them; others check per row.
  - *`context.assigned`* (0046) is computed per resource, so a plan leaves it unknown: a filter
    that depends on it has an `unknown` node there, never a guess.
- **Contained or unregistered principals get `always_denied`**, as their checks are denied.
- **Freshness:** a plan carries its policy bundle version, the principals snapshot version (which
  risk scores move), and a `max_age`. A plan is cached by its inputs and dropped when either
  version or the containments change. A paginating app sends the first page's `plan_qid` with each
  next page and gets the same plan back, pinned, until it's older than `max_age` or the principal
  is contained.
- **Plans are recorded as their own record type**, `PlanRecord`, keyed by a hash of the filter, in
  their own topic and table. Policy dry-run replays decisions, not plans, and says how many plans
  it skipped. The app reports what a plan returned (the count and type, or the IDs) to awzy-intake
  as an access of kind `listed`, with the plan's QID, so volume anomalies still see bulk reads.

Rejected: an ID set from awzy-context for every plan (AuthZEN's Resource Search). It's the
fallback for small sets of awzy's own resources, not a filter an app can join against. Rejected:
evaluating `context.assigned` as false in plans. A forbid that fires only when someone is assigned
would then be skipped, and the filter would show rows a check denies.

## Consequences

- The filter never shows a row that a check of the same request would deny, given the same
  attributes. A property test holds the engine to this over random entities and policies.
- Cedar's partial evaluation is an experimental feature, so the PDP pins the Cedar version it was
  tested with; upgrading Cedar re-runs the property test.
- An app's mapper decides what it can express. The reference mapper turns filters into Postgres SQL
  and reports the parts it can't map.
