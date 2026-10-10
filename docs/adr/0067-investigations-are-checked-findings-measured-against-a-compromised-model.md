# 0067. Investigations are checked findings, measured against a model that obeys every injection

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0048](0048-awzy-bot-on-the-installs-own-model-service.md) (the install's own model
  service), [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (scores, and the LLM
  as a raise-only layer), and [0039](0039-synthetic-organization-for-demo-and-tests.md) (the
  synthetic company as ground truth)

## Context

awzy-reasoner's scores and containment rule are deterministic. They're cheap, and they can't be
talked out of anything, but they can't weigh context either: whether an export at quarter end is
routine, or whether a run of basic questions is a new hire's. The paper's slow path is "AI-run
investigations in minutes", and the platform plan gives them to the reasoner in Phase 4.

Everything an investigation reads can be written by the person under investigation: document
text, agent traces, justifications, even app events. The platform plan's threat model names two
attacks: text that steers the agent to contain someone else, and text that tells it the activity
is routine. A prompt can lower the odds of either, but it can't rule them out, and a model's
behavior changes with every model version.

## Decision

- **An investigation is a model run that ends in a finding, and code decides what the finding
  may do.** The agent runs on the install's own model service (0048), with read-only tools over
  one subject's activity: decisions, events, challenges, and the resources they touched. It must
  end with a finding in a fixed schema: a verdict (`malicious`, `suspicious`, `benign`, or
  `inconclusive`), a risk score, a summary, the evidence it relied on (each item cited by ID), and
  whether it recommends containment.
- **The checks are code, not prompt.**
  - The finding is about the case's subject. A finding about anyone else is discarded.
  - Cited evidence must be evidence the tools returned. Unknown IDs are dropped, and a finding left
    with no evidence becomes `inconclusive`.
  - The risk it can set is never lower than the deterministic score (0056).
  - It can't close a case whose deterministic score is elevated (above 40). A `benign` finding there
    stays open for a person.
  - A containment needs the subject's deterministic score to be elevated already. Otherwise a model
    that follows an injection could get anyone contained by citing their own ordinary decisions.
  - A containment needs evidence from sources the subject doesn't control: PDP decisions,
    challenges, and server-side or cloud audit logs. Documents and agent traces alone are never
    enough. A recommendation without that evidence is dropped.
  - A containment of an administrator, or anyone in the security team, is only ever proposed to a
    person, never applied.
- **Untrusted text is quoted.** Tools return it as JSON string fields marked as untrusted content,
  and the instructions say it's data. That's a mitigation, not a control: the checks above are the
  control.
- **The checks are evaluated in CI against a compromised model.** The eval harness lives with the
  synthetic company (0063), which knows the truth. It builds cases from Larkspur's scenarios and
  ordinary days, and adds adversarial content: documents and traces telling the agent to contain
  someone else, and an attacker's own text telling it to close the case. A scripted model that
  obeys every instruction it reads plays the agent. Every PR must show that even that model causes
  no containment of the wrong person, no containment of anyone the deterministic layer doesn't
  flag, no containment on evidence the subject controls, and no score below the floor.
- **The real model is measured, not gated.** The same harness runs against Bedrock on demand (and
  later on a schedule): the catch rate on the scenarios, false positives on ordinary days, and how
  often the model itself followed an injection. Results are written up in `docs/evals/`. They don't
  block PRs, because they cost money and aren't deterministic.

Rejected: trusting a stronger prompt or a second model to judge the first. Both are useful and
both fail open. Rejected: running the real-model evals on every PR. A change to the checks is what
needs gating, and the compromised model covers that for free.

## Consequences

- The agent can be changed freely (prompt, tools, model version) as long as the compromised-model
  eval passes. What the real-model numbers say about quality is a release decision.
- What an investigation can do is bounded by the checks, so its usefulness grows with the evidence
  sources awzy has that a subject can't write. Today those are PDP decisions, challenges, and
  CloudTrail.
- Still to come: when investigations run (on the reasoner's flags), where findings are stored, the
  client portal's Investigations page and MCP tools, and the per-install token budget.
