# 0061. Ownership and sharing are in the resource entity, and the base policies permit by them

- Status: Accepted
- Date: 2026-10-10
- Implements part of: [0044](0044-pdp-loads-resources-from-awzy-context.md) (the entity it loads)
  and [0046](0046-work-assignment-as-context-assigned.md) (the scenarios work assignment serves)

## Context

The demo plan's step 6 runs the paper's scenarios that challenges answer: the curious contractor
and the first half of the rogue agent. Both expect a *challenge*, and in awzy a challenge is a
`@challenge` forbid in front of something a policy permits. But almost nothing permitted reading
a document. Only the security team (anything) and engineering (public and internal) had a read
permit, so an ordinary read of a team's own document came out as a plain deny. Larkspur's
activity was mostly "would deny" in monitor mode, and the scenarios couldn't be challenged.

awzy-context already stores each resource's owner and its Drive-style sharing (person, group,
domain, or anyone, each with a role) from connectors. The platform plan lists "sharing edges in
the entity" as not yet built. The paper's rogue agent starts from exactly this: "both identities
are authorized... Access allowed" by the static check, and Beyond Zero catches what that misses.

Building the scenarios also exposed a PDP bug. A request that no policy permits, but that a
`@challenge` forbid matches, came out as a challenge. Passing it would still have ended in a
deny.

## Decision

- **The resource entity carries ownership and sharing.** awzy-context adds these, optional in the
  schema:
  - `owner: Human`
  - `shared_people: Set<Human>` and `shared_groups: Set<Group>`: anyone with any role
  - `writer_people` and `writer_groups`: writer and owner roles
  - `shared_with_everyone: Bool`, from an "anyone" share, or a domain share, since a single-tenant
    install's domain is the client's own

  Sharing is **effective**: a resource's own shares plus every ancestor folder's, as in Drive. The
  PDP already loads the chain (0044), so the cost is computing a union. Group IDs are group names
  and people's are userNames, as in the principals snapshot.
- **A bundle published before these attributes still loads.** The PDP probes its schema for each
  optional resource attribute and drops the ones it doesn't declare, as it already did for
  `unclassified`.
- **Base policies permit by them:**
  - `owners-and-shares-read`: a person reads what they own or what's shared with them.
  - `owners-and-writers-write`: owners and writers change it.
  - `service-accounts-read-group-shares`: a service account reads what's shared with its groups.
    Acting for someone, the PDP still checks the person, and the stricter verdict wins (0016), so
    an agent never gets more than both of them could.
  - `owners-approve`: a resource's owner approves challenges on it.
- **The scenarios' patterns are base policies, on top of those permits:**
  - `contractors-outside-their-work`: a member of `contractors` reaching confidential data that
    none of their work links needs the owner's approval.
  - `agents-outside-their-work-justify` and `-approval`: an AI agent acting for someone on
    confidential data outside its work needs that person's justification *and* the owner's
    approval.

  Both use `context.assigned` (0046) and fire only when assignment is known.
- **A challenge only stands in front of something policy allows.** When every forbid that fired
  asks for a challenge, the PDP evaluates once more without the `@challenge` policies. If nothing
  then allows the request, the verdict is deny. The extra evaluation happens only on challenge
  verdicts.
- **Larkspur:**
  - The scenario documents are overshared, as the paper's premise needs: the payments design to
    all of engineering, Quarry Labs' contractors included, and the strategic plan to all of sales.
  - Agents that work for their owners are assignees on their owners' work, as SalesGenie already
    was on Nora's accounts.
  - The scenarios run end to end in a test, through awzy-context, the PDP, larkspur-drive, and
    awzy-gateway. Every scenario step must come out as expected, and no ordinary access may be
    challenged.

Rejected:
- A Larkspur-only demo bundle with broad permits. It isn't how real permissions work, so the
  scenarios would prove less.
- One mixed `Set` of people and groups. Cedar sets are typed, so it's two sets.
- Resolving folder inheritance in Cedar. Policies can't walk parents' attributes; awzy-context can.

## Consequences

- Clients whose bundles define their own read permits keep them. The new permits widen access
  only to what the source system already shares.
- Sharing changes reach the PDP as resources are rewritten, and through the PDP's resource cache
  (30 seconds, 0044). A real change feed comes with Valkey.
- Group names in `contractors-outside-their-work` are the client IdP's. A client without a
  `contractors` group gets nothing from that policy until they name theirs.
- The synthetic Job's service-account registration was rejected by awzy-context ("missing field
  `id`"), because the body had to repeat the ID that the path already carries. The path is now
  the authority.
