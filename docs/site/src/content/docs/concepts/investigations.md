---
title: Investigations
description: A model looks into the principals awzy flags, and code decides what its findings may do.
---

awzy's [risk scores](/concepts/risk/) and [automated containments](/concepts/challenges-and-containment/#automated-containments)
follow fixed rules. They're cheap and can't be talked out of anything, but they can't weigh
context, such as whether an export at quarter end is routine. Investigations add that judgment:
a model, on your cloud's own model service, looks over what a principal did and says what it
makes of it.

Investigations are off until you [turn them on](/guides/optional-features/#investigations).

## When one runs

After each scoring run, every 15 minutes, awzy-reasoner investigates each person or service
account whose score is above 40, or who had a challenge rejected in the last day. It starts with
the highest score, and investigates nobody twice in 24 hours.

## What the model sees

The last 24 hours of awzy's own records about that principal:

- their latest decisions and events, as awzy recorded them;
- the challenges sent to them, and how they were answered;
- the text of up to five documents they opened, cut to 2,000 characters each.

It reads them through two read-only tools, and it can't call anything else. Text anyone could
have written, such as a document's contents, a justification, or an agent's prompt, reaches it
marked as untrusted, with an instruction to treat it as data.

## What a finding may do

The model ends with a finding: a verdict (malicious, suspicious, benign, or inconclusive), a risk
score, a summary, and the evidence it relied on. Then code, not the model, decides what that
finding may do:

- **It's about this principal.** A finding about anyone else, such as one an injected instruction
  steered, is thrown away.
- **Its evidence is real.** Evidence the tools never returned is dropped. A finding left with none
  is inconclusive.
- **It can't lower a score.** Its risk is never below the deterministic score.
- **It can't close a flagged case.** A benign finding about someone scored above 40 waits for a
  person to close it.
- **It can contain only someone already flagged, on evidence they couldn't write.** A
  recommended containment needs a malicious verdict, a score already above 40, and evidence from
  decisions, challenges, or system and cloud audit logs, not from documents or agent traces alone.
  It then goes through the automated-containments mode, so in monitor it's only recorded.
- **Privileged principals are only proposed.** For members of your `it` and `security` groups (you
  can change which), a containment is proposed for a person to decide, never applied.

Every investigation is kept with what the checks changed, so you can see where the model and the
rules disagreed.

## Tested against a model that follows every instruction

awzy's CI runs the investigations against a model scripted to do whatever the evidence tells it.
The test cases come from a synthetic company with known attacks and ordinary days, and plant text
telling the model to contain someone else, to call an attack routine, or to contain the principal
on the planted text alone. Even that model gets none of it past the checks, on every change to
awzy. Real models are measured separately, for how many attacks they catch and how often they
follow an injection.

## Reviewing them

The client portal's [Investigations](/client-portal/#investigations) page lists them. **Needs
review** shows those a person has to decide. Each shows the verdict, the risk next to the
deterministic score, the evidence, what the checks changed, and what became of any containment.
Close one with what your review found. Your own agents can read them through the portal's MCP
tools `list_investigations` and `get_investigation`.

## What it costs

Each investigation is a call to your model service, billed to your cloud account. A monthly token
budget, two million by default, caps them. When it's spent, no new investigation starts until the
next month, and the riskiest principals are the ones that got investigated first.
