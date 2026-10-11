# 0082. Switching enforcement takes a security key, through its own action

- Status: Accepted
- Date: 2026-10-11
- Builds on: [0007](0007-monitor-before-enforce.md) (monitor before enforce),
  [0020](0020-the-portal-is-authorized-by-the-pdp.md) (the portal is authorized by the PDP), and
  [0059](0059-security-keys-for-verification-challenges.md) (security keys)

## Context

Switching an install, or automated containments, between monitor and enforce changes what awzy
blocks for everyone. Any session with `administer` on the enforcement area could do it with one
click, so a stolen session, or an agent working with an admin's own token, could turn enforcement
off. awzy-policy now signs the mode (0080), but signing only protects the stored mode, not who
asks for the switch.

Putting a `@challenge` on `administer` would have challenged every check of it, including the
`viewerCan` the page makes to show the switch, opening a pending challenge each time the page
loaded.

## Decision

- **A `switch_enforcement` action on `PortalArea`,** checked by the client portal only when
  someone switches, in either direction and for either scope, on top of `administer`.
- **The base policies** let the security team's people switch, and ask every switch for a
  verification challenge (`enforcement-switches-need-a-key`). A service account never switches.
- **The portal runs it in place:** the first `setEnforcementMode` returns the challenge's QID and
  switches nothing; the Enforcement page has the admin touch their key, then switches again
  within the challenge's grant.

## Consequences

- An admin needs a security key enrolled before they can switch; the page says so.
- A client can relax it in policy (drop the forbid), as with any base policy, and the decision is
  then theirs.
- A bundle published before this action existed doesn't know it: the switch is refused until the
  current base bundle is published.
