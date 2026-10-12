# 0117. A person's manager and team

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0116](0116-people-carry-what-the-idp-says-about-them.md) (people carry what the IdP
  says about them), [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (peer groups)

## Context

The Beyond Zero paper's world model draws "the graph relationships of the team to other teams"
from the org chart, and its challenges include "requesting manager approval". awzy had neither.
awzy-reasoner guessed a person's team as the smallest group they're directly in, which picks
the wrong group whenever someone is in a small group that isn't their team: the synthetic
company's team leads are in `managers`, which is smaller than their own teams (#356).

## Decision

- **`manager` is a `Human` attribute**, from the SCIM enterprise extension's `manager`. IdPs
  send the manager's SCIM ID, their own ID for them, or a bare string, so awzy-context matches it
  against each person's SCIM ID, then external ID, then user name, and leaves it out when it names
  nobody provisioned, or the person themself.
- **`team` is a `Group` attribute, from groups the client names as teams**
  (`AWZY_CONTEXT_TEAM_GROUPS`, the bootstrap's `team_groups`): group names, each of which may use
  `*`. A person in several team groups is on the one with the fewest direct members. Without the
  setting nobody has a team. Demo installs name the synthetic company's teams.
- **awzy-reasoner compares a person with their team** when they have one, and falls back to the
  smallest group they're in, as before, when they don't.
- **On an `approve` check the PDP loads the person approved for** (`context.approving_for`) from
  the snapshot, so a policy can compare their manager with the approver. Only their entity: risk
  and containment stay the approver's.
- **The base policies don't let managers approve.** In the curious-contractor case the
  contractor's manager is the same vendor's lead, so a blanket manager approval would weaken the
  check the scenario depends on. The docs give the policy for clients who want it.

Rejected: teams from the org chart alone, as the people who share a manager. It needs no
setting, but it puts each team lead among the other leads instead of with their own team, and
people whose IdP has no managers would have no team at all.
Rejected: a team from `department`. Departments are too coarse for a peer group: sales is
several teams with different accounts.

## Consequences

- Policies can name a person's manager and team, and approvals can go to a manager where a
  client's policy says so.
- Risk scores compare people with the team the client declared. A client who sets nothing keeps
  today's behaviour.
- A wrong `team_groups` pattern gives people the wrong peers. The principal's page in the client
  portal shows each person's team, so it can be checked.
- The approval notification doesn't name the manager yet; it still goes to the resource's owner
  and the security team.
