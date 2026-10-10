---
title: Challenges and containment
description: Asking for more than a yes or no, and stopping an accessor everywhere at once.
---

## Challenges

A challenge turns a "no" into "yes, once you've done this":

| Challenge | Who answers | How |
| --- | --- | --- |
| **Justification** | The person | Says why, in the client portal. |
| **Verification** | The person | Signs in with an enrolled security key (WebAuthn). No agent can pass it. |
| **Approval** | Whoever policy allows to approve the resource: a person, or an agent you name | Approves or rejects in the client portal. |

The person is whoever the request is for: the signed-in human, the person an agent acts for, or an
unattended service account's owner. An approval never comes from the request's own subject or the
person it's for.

### Agents as approvers

As agents take on more work, you may want an overseer agent to approve routine requests instead
of asking a person each time. Name it in two policies: one lets it see challenges, the other lets
it approve.

```cedar
permit (principal == ServiceAccount::"overseer", action == Action::"view",
        resource == PortalArea::"challenges");
permit (principal == ServiceAccount::"overseer", action == Action::"approve", resource)
when { resource.sensitivity == "internal" };
```

It approves through the client portal's API with its own token, and the decision records it as the
approver. Two limits hold whatever your policies say: an agent never approves a request its own
owner answers for, since that would be the owner approving themselves, and `crown_jewels` always
need a person. Verification and justification always come from a person, so every chain of agents
ends at someone accountable.

An enforcement point that gets a challenge verdict returns it to the caller with a challenge ID.
The person answers it, and the caller retries the same request. A passed challenge is good for a
short window; a rejected or expired one becomes a deny.

## Containment

A containment revokes one person's or agent's access everywhere, until someone lifts it. Every
PDP applies it within seconds, and enforcement points that are cut off from the PDP never replay
a cached allow for a contained accessor. Each containment records who made it and why.
