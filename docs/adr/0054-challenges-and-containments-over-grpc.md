# 0054. awzy-challenge serves challenges and the containment feed over gRPC; Valkey waits

- Status: Accepted; amended by [0065](0065-agents-approve-people-verify.md) (service accounts may approve) and [0119](0119-a-pdp-outlasts-its-control-plane.md) (the last containment list serves for a bounded time)
- Date: 2026-10-09
- Implements part of: [0003](0003-one-pdp-for-every-access-path.md) (the PDP's challenge verdict
  gets a service behind it)

## Context

Since the beginning the PDP has returned challenge verdicts (`@challenge` on a `forbid`), but
nothing could act on them. The gateway told callers that challenges weren't available, and the
portal refused them. Phase 3 of the platform plan needs challenges that a person can pass,
containments that revoke an accessor's access, and both reaching every PDP fast: a containment
within 2 seconds, and a passed challenge honored on the next retry within 2 seconds.

The plan puts short-lived grants and the containment list in Valkey, with a pub/sub feed to the
PDPs. Nothing uses Valkey yet ([0044](0044-pdp-loads-resources-from-awzy-context.md) deferred it
for resources too), and a first slice doesn't need a second store to meet those targets.

## Decision

- **awzy-challenge owns challenges and containments in its own Postgres database**, served as
  `awzy.challenge.v1.ChallengeService` (gRPC). A challenge is for one exact request: subject,
  `on_behalf_of`, action, and resource. Passing it grants that request alone, for
  `AWZY_CHALLENGE_GRANT_SECS` (15 minutes by default).
- **The PDP opens challenges, not the enforcement points.** On an enforced challenge verdict, a
  PDP configured with `AWZY_CHALLENGE_ADDR` calls `Resolve`. The call returns which required kinds
  this request has already passed, and the pending challenge for the rest. Retries share one
  pending challenge. If some kinds have passed, the PDP decides again with the `forbid` policies
  that ask only for those kinds set aside. A passed challenge therefore never lifts a plain deny
  or a challenge of another kind, and never grants what no policy permits. The response carries
  `challenge_id`, so every PEP gets challenges without new code. Only challenge verdicts make
  the call, and in monitor mode nothing is opened ([0007](0007-monitor-before-enforce.md)). If
  awzy-challenge doesn't answer within a second, the verdict stays a challenge with no ID.
- **Who answers.** The challenge goes to the human subject, the person a service account acts
  for, or the service account's `owner`. A justification must come from that person and be at
  least 20 characters. An approval must come from someone else: never the person asking, the
  person an agent acts for, or the person the challenge went to. The PDP must also allow the
  approver to `approve` the resource, a new Cedar action whose base policy lets the security
  team approve. A rejection denies the request (`challenge-rejected`) until it expires
  (`AWZY_CHALLENGE_PENDING_SECS`, 24 hours by default, also how long a challenge waits for an
  answer).
- **Containments** are added and lifted by a person (`created_by`, `lifted_by`, a reason), and
  rows are kept as the audit trail. Each change bumps a feed version and calls `pg_notify` in the
  same transaction. Each awzy-challenge replica reloads on notification, and every 5 seconds as
  a backstop.
- **The containment feed is a gRPC stream.** `WatchContainments` sends the whole active list on
  connect and on every change, with a heartbeat every 500 ms between. A PDP holds the list in
  memory and isn't ready until the first list arrives. A contained accessor, or a delegated
  request for a contained person, is denied before any policy is evaluated, and the containment
  ID stands in for the policy. If no heartbeat arrives for 2 seconds, the PDP asks awzy-challenge
  directly on each check. If that fails, the check fails as unavailable, never as "not
  contained."

Valkey comes later: when PDP replicas outnumber what one stream per PDP can serve, or when grants
need to be read without a call. Then the stream becomes Valkey pub/sub and the PDP side stays
the same.

Rejected: enforcement points calling awzy-challenge themselves. Every PEP would need the same
code, and a PEP could open a challenge for a request the PDP never decided. Rejected: passing
the passed kinds into Cedar context for policies to test. Every client policy would have to
remember an `unless`, and one that forgot would never honor a passed challenge.

## Consequences

- A PDP configured with awzy-challenge depends on it to start, as it does on awzy-policy.
  Afterwards only challenge verdicts, and checks made while the feed is stale, wait on it.
- Containments follow the install's enforcement mode for now: in monitor a containment is a
  "would deny." A separate mode for automated containments waits for awzy-reasoner, which will
  request them.
- WebAuthn verification and biometric checks aren't built. A challenge that needs one stays
  pending until it expires. Nor are the LLM check of a justification against the resource,
  prompts over Slack, Teams, or email, lifting a containment by challenge, or the client portal
  pages.
- Bundles published before `approve` existed can't authorize approvers. Approvals are refused
  until the client republishes on the new schema.
- The decision log doesn't record the challenge ID yet. Joining a decision to its challenge
  needs a ClickHouse column.
