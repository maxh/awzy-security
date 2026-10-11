---
title: How awzy decides
description: One check per action, against Cedar policy, the principal, and the resource.
---

Every request an enforcement point sends is one **check**: a *subject* (who), an *action* (what),
and a *resource* (on what), with *context*.

```text
subject:  ServiceAccount::"sales-genie"   acting for Human::"bob"
action:   read
resource: Resource::"q3-strategy-deck"
context:  { "human_present": false }
```

awzy-pdp fills in what it knows: the person's groups and status from your IdP, the resource's
sensitivity and owner from awzy-context, the principal's current risk score, and whether there's
open work assigned to them on that resource. Then it evaluates your Cedar policies.

## Verdicts

| Verdict | Meaning |
| --- | --- |
| **Allow** | Go ahead. |
| **Deny** | No: either a `forbid` policy matched, or no `permit` did (deny by default). |
| **Challenge** | Allowed once every listed challenge is passed: a justification, a security key, or an approval. |

A challenge comes from a `forbid` policy annotated with `@challenge(...)`. Passing the challenge
lifts that one policy for that request, and the PDP decides again.

## Every decision is explained

Each decision has an ID (`decision_…`) and is logged with the policies that produced it. In the
client portal, any decision opens to show the request, the context, and the source of each
policy that decided it.
