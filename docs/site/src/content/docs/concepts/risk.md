---
title: Risk scores
description: How awzy scores each person's and agent's risk, and how policy uses it.
---

awzy-reasoner scores each person and service account from 0 to 100, every 15 minutes, from their
last 24 hours of activity. Each signal adds points up to its own limit, and the total is capped
at 100:

| Signal | Points | At most |
| --- | --- | --- |
| How many resources they opened, against their own usual day | 15 for each multiple above usual | 50 |
| The same, against their team's median (their team is the smallest group they're in) | 15 for each multiple above it | 50 |
| Requests denied | 5 each | 20 |
| **Unfamiliar territory:** resources neither they nor anyone on their team had opened in the 30 days before, or alongside them that day | 10 for each one past the first two | 50 |

A few guards keep the comparisons fair. Their own baseline counts only after 5 active days, a team
needs 3 other active members to be compared with, and "usual" is never taken as fewer than 5
resources a day, so a quiet day that doubles is still quiet. A resource more than 100 principals
have opened is familiar to everyone.

No signal alone can reach 80, the score where everything is challenged, though unfamiliar
territory alone can make risk elevated. Challenges don't count against anyone, so a high score
can't feed itself. Each score is stored with its reasons, such as "opened 14 resources in the
last day; usually 4 a day".

## How policy uses it

The scores travel with the directory to every PDP, and each check uses its own principal's score
as `context.risk_score`. When an agent acts for a person, the agent and the person are each
judged on their own score. The base policies:

| Risk above | What happens | Policy |
| --- | --- | --- |
| 40 | A person calling a `crown_jewels` MCP tool (a high-risk operation, such as resetting credentials or exporting the directory) must use their security key. An agent working with their credentials can't | `elevated-risk-high-risk-operations-verify` |
| 50 | A service account acting for someone who isn't there needs an approval | `absent-human-approval` |
| 80 | Everything needs a security key | `high-risk-verify` |

The 40 threshold catches the paper's "suddenly foolish administrator": someone opening other
teams' material more than usual, whose risky operations then need proof that it's really them.

The scores are deterministic: the same activity always gives the same score. Later LLM-driven
investigations may raise a score, never lower it below this one.
