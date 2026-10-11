# 0095. Support sessions: a client admin approves, and staff sign in with a corp-signed token

- Status: Accepted
- Date: 2026-10-11
- Implements part of: [0009](0009-no-vendor-access-to-client-data.md) (support sessions)

## Context

0009 allows awzy support into an install only through sessions that are read-only, time-boxed,
scoped to named portal areas, approved by a client admin each time, and audited. None existed:
support saw only the heartbeat and a diagnostics bundle an admin sends. The deployment plan left
open how a staff member signs in to the client portal during a session: federation with awzy's
identity, or a guest account in the client's IdP.

## Decision

- **A client admin decides in the client portal.** awzy staff ask in the corp portal (areas,
  reason, duration); the request reaches the install through awzy-reporter, the only thing that
  talks to the corp portal, and awzy-challenge keeps it. An admin approves (some or all of the
  areas, for at most the time asked) or rejects it on the Support page, and can end an active one.
  Approving goes through `switch_enforcement`, so the base policies ask for a security key.
- **Staff sign in with a token the corp portal signs** for one session and one install (ES256,
  `iss` `awzy-corp-portal`, `aud` `awzy-support:<install QID>`, the session's QID as `sid`). The
  client portal trusts the corp portal's support public key, set in the chart, and accepts a token
  only while awzy-challenge says its session is approved, unexpired, and that staff member's. The
  client does nothing in its IdP, and ending the session ends access at the next request.
- **Read-only is enforced twice.** The portal refuses anything but `view` on an approved area
  before asking the PDP, and names the approved areas in the check's context
  (`support_areas`); the base policies permit `view` on exactly those and forbid everything else
  for any check that carries them. A staff member is `Human::"awzy-support:<name>"`, a person the
  directory doesn't know, in no group. A bundle whose schema predates `support_areas` loses the
  key and denies the checks; the portal's own refusal still holds.
- **Everything is recorded.** Each operation a staff member runs is recorded against the session
  before it runs, and an operation that can't be recorded doesn't run. The Support page shows each
  session's history and how many operations it made.

## Consequences

- No standing access and no IAM role in the client's account: without a fresh approval, a token
  is refused.
- The corp portal's support signing key must be guarded like the release key; anyone holding it
  can mint tokens, though only for sessions a client approved, and only until they end.
- The staff member's browser must reach the client portal's URL, which the client controls.
- Requests and session states travel in the heartbeat's answer and the next heartbeat, so a
  request takes up to a minute to arrive.
