# 0066. awzy-reasoner contains people automatically, under their own enforcement mode

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0054](0054-challenges-and-containments-over-grpc.md) (containments),
  [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (risk scores), and
  [0007](0007-monitor-before-enforce.md) (monitor before enforce)

## Context

The paper's rogue agent ends with a containment. SalesGenie, prompted by a rep whose risk is
already rising, reaches for the strategic plan and is challenged twice. The challenges aren't
cleared, and a slower look at the rep's recent actions finds logs of them trying to exfiltrate
data. Then "the user is contained", and "in the vast majority of cases, the decision to contain
will be autonomous."

Containments exist (0054), but only a person makes or lifts one, in the client portal. The
platform plan gives automated containments their own enforcement mode: they start in monitor,
and a client switches them to enforce once the share lifted on human review is low enough.

## Decision

- **awzy-reasoner contains a person when three things hold in the same day.** In the 24 hours
  before its run:
  - their risk score is elevated (above 40, as computed in the same run);
  - a challenge sent to them was rejected, whether it was for their own request or for an agent's
    request on their behalf;
  - they, or an agent acting for them, attempted to move data out: an event whose action is
    `download`, `export`, `send_external`, or `share_external`, whatever its outcome.

  Each of these happens on its own in ordinary work. Together they're the paper's pattern, and a
  containment costs much more than a challenge, so the first rule is narrow and deterministic.
  The reason it records lists the evidence.
- **It contains the person, not the agent.** A contained person's delegated requests are denied
  too (0054), so every agent they prompt stops with them. Containing service accounts on their own
  evidence comes later.
- **Automated containments have their own scope, `automated-containments`, in awzy-policy.** It
  has the same append-only history as the install's mode and starts in monitor. The reasoner reads
  the mode on every run. If it can't read it, it contains no one.
- **In monitor, a containment is recorded but not applied.** awzy-challenge stores it with
  `monitor` set, shows it in the client portal as "would contain", and never sends it to PDPs. The
  security team either contains the person for real or dismisses it by lifting it. A person's own
  containments are always enforced, as before.
- **The reasoner acts as itself.** It calls awzy-challenge's `Contain` with its own service
  identity (0055), recorded as `AwzyService::awzy-reasoner`. It never lifts a containment. If a
  containment for that person, enforced or monitored, was lifted in the last day, it doesn't
  contain them again: a person's review stands until new evidence comes in, a day later.

Rejected: letting awzy-challenge look up the mode instead of the reasoner. A reasoner that can
write risk scores can already challenge anyone, so trusting it to pass the mode adds little, and
awzy-challenge would need a client for awzy-policy just for this. Rejected: an LLM deciding
whether to contain. Investigations come later, may raise a score, and may propose a containment,
but this deterministic rule stays as a floor they can't lower (0056).

## Consequences

- The client portal's Enforcement page gets a second switch, for automated containments, and its
  Challenges page marks containments made by awzy-reasoner and the ones it would have made.
- The quality measure is the share of automated containments lifted on review. Its target, and
  when a client should switch to enforce, wait for design-partner data (platform plan, Phase 4).
- awzy-reasoner now calls awzy-policy and awzy-challenge over gRPC, and awzy-challenge allows its
  identity to call `Contain`.
- Rejected challenges are read from awzy-challenge, and exfiltration attempts from ClickHouse's
  events. Until awzy-intake takes in DLP and browser logs, those attempts come only from app
  events that use these action names.
