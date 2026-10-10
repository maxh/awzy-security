---
title: Challenges and containment
description: Asking for more than a yes or no, and stopping an accessor everywhere at once.
---

## Challenges

A challenge turns a "no" into "yes, once you've done this":

| Challenge | Who answers | How |
| --- | --- | --- |
| **Justification** | The person | Says why, in the client portal. |
| **Verification** | The person | Signs in with an enrolled security key (WebAuthn). |
| **Approval** | Someone policy allows to approve the resource | Approves or rejects in the client portal. |

An enforcement point that gets a challenge verdict returns it to the caller with a challenge ID.
The person answers it, and the caller retries the same request. A passed challenge is good for a
short window; a rejected or expired one becomes a deny.

## Containment

A containment revokes one person's or agent's access everywhere, until someone lifts it. Every
PDP applies it within seconds, and enforcement points that are cut off from the PDP never replay
a cached allow for a contained accessor. Each containment records who made it and why.
