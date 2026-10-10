---
title: Risk scores
description: How awzy scores each person's and agent's risk, and how policy uses it.
---

awzy-reasoner scores each person and service account from 0 to 100, every 15 minutes, from their
recent activity:

- what they opened in the last 24 hours against their own usual day,
- against their team's median (their team is the smallest group they're in), and
- how many of their requests were denied.

Each comparison adds at most 50 points, so one unusual signal alone stays below the challenge
threshold. Challenges don't count against anyone, so a high score can't feed itself.

The scores travel with the directory to every PDP, and each check uses its own accessor's score
as `context.risk_score`. When an agent acts for a person, the agent and the person are each
judged on their own score. The base policies challenge with a security key above 80, and ask for
an approval above 50 when an agent acts for someone who isn't there.

The scores are deterministic: the same activity always gives the same score. Later LLM-driven
investigations may raise a score, never lower it below this one.
