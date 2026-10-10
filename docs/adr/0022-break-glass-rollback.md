# 0022. Break-glass rollback with admin keys enrolled at install

- Status: Accepted
- Date: 2026-10-09

## Context

Policy decides who can use the client portal (0020), so a bad bundle can lock out everyone who
could fix it. Recovery can't depend on the portal, the PDP, or the IdP, because any of them may
be what's broken.

## Decision

- At install, each client admin's public key (ECDSA P-256) is enrolled with awzy-policy
  (`AWZY_POLICY_BREAK_GLASS_KEYS_FILE`).
- The `awzy-break-glass` CLI signs a rollback request with an admin's private key and sends it
  straight to awzy-policy. The request names the target version (default: the bundle published
  before the active one), a timestamp, a random nonce, and a reason.
- awzy-policy accepts it only if an enrolled key signed it, it is under 5 minutes old, and its
  nonce hasn't been used. It republishes the target as a new version (signed at publish, 0021) and
  records the event (`break_glass_events`: key, reason, versions) in the same transaction.
- Break-glass can only restore a bundle that was published before. It can't publish new policy or
  change the enforcement mode.

## Consequences

- It works when the IdP, the PDP, or the portal is down: it needs only network access to
  awzy-policy and an admin's key.
- Admin keys are files for now. Hardware security keys hold P-256 keys too, which is the intended
  next step, so a stolen laptop isn't enough.
- Not yet: alerting every admin on each use, and enrolling or revoking keys without a restart.
- This replaces the plan's earlier idea of a built-in break-glass policy outside the editable
  bundle. A signed rollback is simpler, and it doesn't put a permanent permit in every bundle.
