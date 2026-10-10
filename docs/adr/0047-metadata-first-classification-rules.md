# 0047. Sensitivity classification starts with rules: label, content markings, then names

- Status: Accepted
- Date: 2026-10-09
- Implements part of: [0039](0039-synthetic-organization-for-demo-and-tests.md) (demo plan step 4)

## Context

Policies decide on a resource's sensitivity tier, but most resources arrive without one: in the
synthetic company about 80% of documents have no label, as in real drives. Until now the PDP
treated all of those as `internal` and `unclassified` (0044). The platform plan says to
"classify cheaply first": metadata, then a model only for what metadata can't place, within the
client's LLM budget. awzy has no model provider wired up yet (`libs/llm` is a trait).

## Decision

- A pure crate, `authz/libs/classify` (`awzy-classify`), places a resource from what a
  connector sends, in order of trust:
  1. **Label:** the connector's `sensitivity`, or a label whose text names a tier ("Highly
     Confidential", "General").
  2. **Content markings** in the text excerpt ("Strictly confidential", "Approved for external
     distribution", "For … employees"). The connector already sent the excerpt, so reading it
     costs nothing. "Content only when metadata can't decide" applies to the model, which is
     what costs money.
  3. **Names and folders:** keywords for kinds of document that are nearly always one tier
     ("board", "payroll", "contract"). They never make anything `public`: the first scoring run
     called 135 internal documents named "case study", "FAQ", or "one-pager" public. Calling a
     document public is the costliest mistake, so it takes a label or a marking.
- **A label is never lowered.** Content marked higher raises it, and the reason records both.
  Mislabeling is far more often too low than too high, and a too-high tier costs a challenge,
  not a leak.
- **Folders are placed only by a label.** A folder named "Board" says what it holds, not that
  listing it is reading board papers. Its name still counts for the documents in it.
- awzy-context runs it in the background. It looks at each resource once per version (a write
  moves `updated_at`, which re-queues it) and stores the tier, what decided it, a one-line
  reason, and when. Policies see the **higher** of the connector's tier and the classifier's,
  so a stale classification can only be too high, briefly.
- What nothing places stays unclassified (`internal` and `unclassified: true`) for the model
  step, which comes with a model provider.
- awzy-synthetic's tests score the rules against the generator's ground truth, with text and
  with metadata only, and assert that nothing more sensitive than `public` is ever called
  `public`.

Rejected: classifying synchronously on write. A document often arrives before its folders, and
the batch API would slow down with every rule we add. Rejected: letting the classifier lower a
label. A client's label is a decision someone made; raising it is a finding, lowering it would
be a leak.

## Consequences

- On generated text, the rules look perfect, because every generated document carries its
  marking. Real documents are messier ("What synthetic data won't tell us" in the demo plan).
  The metadata-only score is the more honest number until real data or the model arrive.
- Keyword lists are English and Larkspur-shaped. Clients will need their own; they're constants
  for now.
- A new or changed resource is classified within about five seconds. Until then it has its
  label's tier or the default.
- Each resource in a pass costs one recursive query for its folders.
