---
title: Monitor, then enforce
description: Every install starts by recording verdicts without applying them.
---

A new install starts in **monitor** mode. awzy decides every request and records the verdict,
but enforcement points allow everything and send no challenges. Denials show up as "would deny".

This is how you find out what your policies would do before they do it. The client portal's
Enforcement page shows how many requests would have been denied or challenged over the last day,
and the most recent ones.

When the numbers look right, an admin switches the install to **enforce**. Every PDP picks the
change up as soon as it's made, usually within a second. Switching back is one click, and every switch is
recorded with who made it.

Before publishing a policy change, a **dry run** replays the last week's requests under the
current policies and the draft, with today's people and resources, and lists every verdict that
would change.
