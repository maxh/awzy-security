# 0127. A starting PDP is never behind its peers' policy, and a scoped mode is never dropped

- Status: Accepted
- Date: 2026-10-15
- Amends: [0080](0080-signed-enforcement-modes-and-grants-per-policy.md) (what a PDP does with a
  scope the list leaves out) and [0119](0119-a-pdp-outlasts-its-control-plane.md) (a starting PDP
  asks its peers even when awzy-policy answers)

## Context

Bundles and enforcement modes are signed so that someone who can write to awzy-policy's
Postgres can't change what PDPs enforce (0021, 0080). A PDP refuses anything unsigned, and
anything older than what it holds. The P model of policy sync (0126) gave that writer the moves
the database allows: setting the active bundle back, deleting mode switches, forging rows. It
found two ways through:

- **A deleted scoped mode was dropped.** A PDP replaced its scoped modes with the list awzy-policy
  sent. Deleting a tool's switches took the tool back to the install's mode on every PDP, monitor
  included, with nothing signed changed.
- **A starting PDP had nothing to be newer than.** With the install's switches deleted,
  awzy-policy answers "never switched": monitor, unsigned, which a PDP accepts while it holds no
  signed mode. Every PDP that restarted then ran in monitor, and likewise on an older signed
  bundle set active again. Pods restart on every rollout.

## Decision

- **A scope awzy-policy leaves out keeps its mode.** Switches are append-only, so a scope that
  was switched can't legitimately disappear. The PDP keeps the last signed switch it applied and
  passes it to starting peers.
- **A starting PDP takes its running peers' policy before its first sync.** It asks up to five
  ready peers (`PdpPeerService.GetState`, 0119), within 5 seconds, and applies the newest bundle
  and modes that verify. awzy-policy's answers must then be no older, as for any running PDP.
  With no peer running, as on a new install, it starts from awzy-policy alone.

## Consequences

- Tampering with awzy-policy's database can't move a running PDP backwards, or start a PDP
  behind the ones running. It can still stop new policy from reaching them, which fails policy
  syncs and fires the policy-freshness alert.
- A PDP starting while every other PDP is down still trusts awzy-policy alone, as before.
- A PDP's start waits up to 5 seconds more when peers are listed but don't answer.
- Two switches of one scope within the same second compare equal, so either can be replayed over
  the other. Times stay whole seconds, since they're in the signed payload.
