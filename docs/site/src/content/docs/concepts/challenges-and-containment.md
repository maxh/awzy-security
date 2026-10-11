---
title: Challenges and containment
description: Asking for more than a yes or no, and stopping a principal everywhere at once.
---

## Challenges

A challenge turns a "no" into "yes, once you've done this":

| Challenge | Who answers | How |
| --- | --- | --- |
| **Justification** | The person | Says why, in the client portal. |
| **Verification** | The person | Touches an enrolled security key (WebAuthn). No agent can pass it. |
| **Approval** | Whoever policy allows to approve the resource: a person, or an agent you name | Approves or rejects in the client portal. |

The person is whoever the request is for: the signed-in human, the person an agent acts for, or an
unattended service account's owner. An approval never comes from the request's own principal or
the person it's for.

Today the client portal lists requests waiting for approval only to those who may `administer`
the challenges area: your security team, under the base policies. Someone else whom policy lets
approve, such as a resource's owner, is told about the request when Slack is connected, but
can't open it in the portal yet; they ask the security team, who can approve it.

### Agents as approvers

As agents take on more work, you may want an overseer agent to approve routine requests instead
of asking a person each time. Name it in two policies: one lets it find what's waiting, the other
lets it approve.

```cedar
permit (principal == ServiceAccount::"overseer", action == Action::"administer",
        resource == PortalArea::"challenges");
permit (principal == ServiceAccount::"overseer", action == Action::"approve", resource)
when { resource.sensitivity == "internal" };
```

Finding waiting requests takes `administer` on the challenges area, which also lets it see every
challenge and contain or lift principals. Grant it only to an agent you'd trust with that, or give
it challenge QIDs another way and grant only `approve`.

It approves through the client portal's API with its own token, and the decision records it as the
approver. Two limits hold whatever your policies say: an agent never approves a request its own
owner answers for, since that would be the owner approving themselves, and `crown_jewels` always
need a person. A token that acts for someone can't answer or approve anything, so an agent with
its own identity can never pass a challenge meant for its person.

An agent that runs with a person's own token is a different matter: to awzy it is that person,
and can answer a justification or approve as them. Only a security key proves a person was
there. See [An agent using a person's own credentials](/concepts/principals-and-delegation/#an-agent-using-a-persons-own-credentials).

An enforcement point that gets a challenge verdict returns it to the caller with a challenge ID.
The person answers it, and the caller retries the same request. A passed challenge is good for a
short window; a rejected or expired one becomes a deny.

## Containment

A containment revokes one person's or agent's access everywhere, until someone lifts it. Every
PDP applies it within seconds, and enforcement points that are cut off from the PDP never replay
a cached allow for a contained principal. Each containment records who made it and why.

A contained person's agents are stopped with them: a request an agent makes for someone who is
contained is denied too.

### Automated containments

awzy-reasoner contains a person on its own when three things happen to them in the same 24 hours:

1. their [risk score](/concepts/risk/) is elevated, above 40;
2. a challenge sent to them was rejected, whether for their own request or for an agent acting
   for them;
3. they, or an agent acting for them, tried to move data out: an event whose action is
   `download`, `export`, `send_external`, or `share_external`, as your apps report it through
   awzy-intake.

Each of these happens in ordinary work. The containment comes from all three together. It records
the evidence as its reason, and it's made by `AwzyService::awzy-reasoner`, so you can tell it from
one a person made. awzy-reasoner checks every 15 minutes, never lifts a containment, and won't
contain someone again within a day of a person lifting or dismissing one.

**They start in monitor.** Automated containments have their own mode, apart from the
install's, set on the client portal's Enforcement page:

- **Monitor** (the default): the containment is recorded but not applied. It appears on the
  Challenges page as **Would contain**, where you can contain the person yourself or dismiss the
  record. PDPs never see it.
- **Enforce:** the containment applies at once, everywhere, until someone lifts it.

Leave it in monitor until the would-contains you've reviewed are ones you'd have made.
[Investigations](/concepts/investigations/) can recommend containments too, and those go through
the same mode.
