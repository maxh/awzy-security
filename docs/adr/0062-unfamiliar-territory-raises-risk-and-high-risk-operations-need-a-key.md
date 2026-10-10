# 0062. Unfamiliar territory raises risk, and high-risk operations at elevated risk need a security key

- Status: Accepted
- Date: 2026-10-10
- Builds on: [0056](0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md) (risk scores) and
  [0059](0059-security-keys-for-verification-challenges.md) (security-key challenges)

## Context

The paper's suddenly foolish administrator logs in as usual, looks up basic questions about the
system that anyone experienced would know, and then runs a host of operations in rapid
succession, as an agent working with their credentials would. Slow reasoning turns the questions
into a risk attribute. The fast path sees that attribute together with a high-risk operation and
asks for the person's security key, which an agent can't touch.

awzy had the pieces at either end: deterministic risk scores (0056) and security-key challenges
(0059). What was missing:

- **A signal for the questions.** The reasoner's first scores compare how much someone opens with
  their own history and their team's. An administrator reading a handful of introductions is an
  ordinary amount of reading, so nothing rose.
- **A policy for the combination.** `high-risk-verify` challenges everything above 80. Below that,
  nothing used risk for a person, and nothing marked an operation as high-risk.

## Decision

- **The reasoner counts unfamiliar resources.** For each principal, it counts the resources it
  opened in the last day that neither it nor anyone on its team had opened in the 30 days before,
  or alongside it that day. A principal with no team is compared only with itself. A resource
  with more than 100 openers is familiar to everyone, which also bounds what ClickHouse returns.
  Only resources count, not tool calls or portal areas: the question is whose material it is.
- **It's scored on its own scale.** Two a day are allowed, since the odd document from another
  team is normal. Each one past that adds 10 points, up to 50. Alone, that makes a principal's risk
  elevated, but never enough to reach the threshold of 80 where everything is challenged.
- **Elevated means above 40.** A new base policy, `elevated-risk-high-risk-operations-verify`,
  turns a person's call of a `crown_jewels` MCP tool into a security-key challenge when their risk
  is above 40. It's written for `Human`, so a delegated request is caught by the person's check
  (0016).
- **A tool's sensitivity is its operation's risk.** No new attribute: the gateway already
  configures each tool's sensitivity, and a high-risk operation (resetting credentials, turning off
  MFA, exporting the directory) is `crown_jewels`.
- **IT calls any tool.** `it-calls-any-tool` permits the `it` group to call any MCP tool, because
  high-risk operations are its work. The challenge sits in front of that permit.

## Consequences

- An administrator who is learning a new area is challenged on high-risk operations for a day.
  That costs a key touch, not a denial, which is the paper's design.
- People whose job is to roam (security, auditors) open unfamiliar material every day and score on
  it every day. Policies today only use elevated risk for high-risk operations, which they rarely
  run. If that's noisy, the fix is a per-principal baseline of unfamiliarity.
- "Rapid succession" isn't a signal yet. ADR 0016's automation score, from machine-speed patterns,
  would add one.
- The demo's synthetic company gains `larkspur-admin`, a stand-in MCP server with those operations,
  which IT runs as routine and the scenario runs at elevated risk.
- A demo that replays the same scenario day after day makes Lee's earlier reads familiar to him,
  and his score stops rising. Tests and single runs are unaffected.
