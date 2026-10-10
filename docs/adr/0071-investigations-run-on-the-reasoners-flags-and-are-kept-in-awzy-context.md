# 0071. Investigations run on the reasoner's flags, within a monthly budget, and are kept in awzy-context

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0067](0067-investigations-are-checked-findings-measured-against-a-compromised-model.md)
  (what an investigation is and what its finding may do),
  [0066](0066-the-reasoner-contains-people-automatically-in-its-own-mode.md) (automated
  containments), and [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (risk scores
  in awzy-context)

## Context

0067 settled what an investigation is: a model run over one subject's evidence that ends in a
finding, which code checks. It left open when one runs, what it reads in a real install, where
the result is kept, and what it costs. Each investigation is a model bill in the client's own
account (platform plan, LLM budget).

## Decision

- **awzy-reasoner opens them on its own flags.** After each run's scores and containments, it
  investigates every person or service account whose score is above 40 or who had a challenge
  rejected in the last day, highest score first. Nobody is investigated twice in 24 hours.
- **A monthly token budget caps them.** `AWZY_REASONER_INVESTIGATION_TOKENS` (2,000,000 by
  default) is the most an install spends in a calendar month (UTC), counted from the stored
  investigations' usage. When it's spent, no new investigation starts until the next month, and
  the reasoner logs a warning at 80 percent. Highest score first means the riskiest are the ones
  investigated when budget is short.
- **The case comes from awzy's own records.** For the 24 hours before the run:
  - the subject's PDP decisions and events from ClickHouse, the latest 60 of each, as decisions,
    server logs, or cloud audit logs by the event's source;
  - the challenges sent to them, from awzy-challenge, with their justifications as untrusted
    text;
  - the text of up to five resources they opened, from awzy-context, at most 2,000 characters
    each, as untrusted documents.

  A principal in `AWZY_REASONER_PRIVILEGED_GROUPS` (`it` and `security` by default, nested
  groups included) is privileged: a containment is only proposed to a person (0067).
- **Investigations are kept in awzy-context**, next to the risk scores they're about, in an
  `investigations` table. awzy-reasoner writes them with its service identity (0055), the same
  as scores. The client portal reads them later. Each one keeps the subject, why it was opened,
  the deterministic score, the checked outcome (verdict, risk, summary, cited evidence,
  containment, what the checks changed), the model's own finding, the model, the token usage,
  and when it ran. Its ID is a QID, `investigation_<uuid>`.
- **The outcome acts through what already exists.** A recommended containment goes through the
  `automated-containments` mode (0066), so in monitor it's recorded and not applied, and its
  reason cites the investigation. A proposed containment, or a benign finding at elevated risk,
  leaves the investigation as `needs_review` for a person. Anything else is `done`.
- **Off until the client turns it on.** `AWZY_REASONER_INVESTIGATIONS=true` enables them with
  Claude on the install's Bedrock and the same model settings as Awzy Bot (0048). The services'
  workload role already allows Bedrock.

Rejected: awzy-reasoner's own Postgres database. It would be one more database in every install
for one table, and the risk scores the investigations are about already live in awzy-context.
Rejected: ClickHouse. A person closes an investigation, so it changes after it's written.

## Consequences

- The client portal's Investigations page, closing an investigation, and the MCP tools come
  next. They read and update this table through awzy-context.
- The budget counts tokens, not money, because prices differ by model and region. The portal can
  show a cost estimate.
- Until DLP and agent traces reach awzy-intake, the independent evidence an investigation can
  cite is PDP decisions, challenges, and CloudTrail.
