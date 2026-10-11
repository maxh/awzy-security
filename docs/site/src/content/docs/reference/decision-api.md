---
title: Decision API
description: Asking awzy-pdp for a decision over gRPC or HTTP.
---

Enforcement points ask awzy-pdp one question: may this principal take this action on this
resource? The same port serves gRPC and HTTP.

## gRPC

`awzy.authz.v1.Authorizer/Check`, defined in
[`authz.proto`](https://github.com/maxh/awzy-security/blob/main/authz/libs/proto/awzy/authz/v1/authz.proto).

| Request field | |
| --- | --- |
| `principal` | Who is acting: `{type: "Human" \| "ServiceAccount", id}`. |
| `action` | What they're doing, e.g. `read`. |
| `resource` | What they're doing it to. |
| `context` | Request context; must match the schema for the action. |
| `on_behalf_of` | The person a service account acts for. |
| `human_present` | Whether a person can answer a challenge now. |

| Response field | |
| --- | --- |
| `decision_qid` | The decision's ID, for explaining it later. |
| `verdict` | `ALLOW`, `DENY`, or `CHALLENGE`. |
| `challenges` | Every challenge that must be passed. |
| `policies` | The policies that decided it. |
| `mode` | `MONITOR` means allow whatever the verdict says. |
| `challenge_qid` | The challenge to answer before retrying. |

An enforcement point that doesn't understand `mode` applies the verdict, which fails safe.

## HTTP

`POST /v1/access/evaluation` takes the same request as JSON. Entities the enforcement point
already knows can come inline, in Cedar's entity shape. Here an AI agent acts for bob while he's
away, at elevated risk:

```sh
curl -s http://awzy-pdp:8080/v1/access/evaluation -H 'content-type: application/json' -d '{
  "principal": {"type": "ServiceAccount", "id": "sales-genie"},
  "on_behalf_of": {"type": "Human", "id": "bob"},
  "action": "read",
  "resource": {"type": "Resource", "id": "design-doc"},
  "context": {"risk_score": 90},
  "entities": [
    {"uid": {"type": "Human", "id": "bob"}, "attrs": {}, "parents": [{"type": "Group", "id": "engineering"}]},
    {"uid": {"type": "ServiceAccount", "id": "sales-genie"}, "parents": [{"type": "Group", "id": "engineering"}],
     "attrs": {"owner": {"__entity": {"type": "Human", "id": "bob"}}, "ai": true}},
    {"uid": {"type": "Resource", "id": "design-doc"}, "attrs": {"sensitivity": "internal"}, "parents": []}
  ]}'
```

```json
{
  "qid": "decision_0199…",
  "verdict": "challenge",
  "challenges": ["verification", "approval"],
  "policies": ["absent-human-approval", "high-risk-verify"],
  "on_behalf_of": {
    "verdict": "challenge",
    "challenges": ["verification"],
    "policies": ["high-risk-verify"]
  },
  "mode": "monitor"
}
```

Both checks challenge, and every challenge that fires is required. `mode` is `monitor` until an
admin switches the install to enforce, so the enforcement point allows the request and the
verdict is recorded as a "would challenge". When awzy-reasoner has scored bob and sales-genie,
their own scores replace the `risk_score` sent here.
