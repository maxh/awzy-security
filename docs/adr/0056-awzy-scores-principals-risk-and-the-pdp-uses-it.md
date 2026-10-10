# 0056. awzy scores each principal's risk, and the PDP uses that score over the caller's

- Status: Accepted
- Date: 2026-10-10

## Context

Policies already test `context.risk_score` (`high-risk-verify` challenges anything above 80), but
the number comes from the enforcement point that asked. Most enforcement points have no idea how
risky a person or agent is, so they send 0, and the policy never fires. The platform plan gives
the job to awzy-reasoner: compare each principal's recent activity with its own history and its
peers, and turn that into risk the PDP can use. The reasoner is slow and batch; the PDP decides
in milliseconds and can't wait on it.

## Decision

- **A score is a number from 0 to 100 per principal** (a `Human` or `ServiceAccount`), with
  short reasons and the time it was computed. awzy-context stores the latest one per principal
  (`risk_scores`), and awzy-reasoner replaces them in batches with `POST /v1/risk`. Writing
  scores needs the context writers' service identity (ADR 0055), the same as writing resources.
- **Scores travel with the principals snapshot.** `GET /v1/principals` gains
  `"risk": {"Human::bob": 90}`, and writing scores bumps the snapshot version, so the PDP picks
  them up on its next sync like any other change to people. No call per decision.
- **The PDP sets `context.risk_score` per principal.** Each check (the agent's own, and the
  person's it acts for) sees its own principal's score, the way `assigned` already works. What
  the enforcement point sent stands only for principals awzy hasn't scored, so an install with
  no reasoner behaves as before.
- **Scores are deterministic first.** The reasoner's first version computes scores from
  ClickHouse activity against baselines, with no LLM. When LLM investigations arrive, they may
  raise a score but not lower it below the deterministic one (platform plan, prompt injection).

## Consequences

- One stale-by-a-sync window: a score lands in the PDP within one principals sync interval.
- The reasons stay in awzy-context for the portal and investigations; the PDP only needs the
  number, so the snapshot carries only that.
- A principal with no score is not "risk 0" but "unknown", which is why the caller's value still
  applies to it. Policies that want to treat unknown principals strictly can't yet tell the
  difference; if one needs to, the PDP can add a `risk_scored` flag.
