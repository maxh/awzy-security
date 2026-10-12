---
title: Decision API
description: Asking awzy-pdp for a decision over gRPC or HTTP.
---

Enforcement points ask awzy-pdp one question: may this principal take this action on this
resource? The same port serves gRPC and HTTP.

## gRPC

`awzy.authz.v1.Authorizer/Check`, defined in
[`authz.proto`](https://github.com/maxh/awzy-security/blob/main/authz/libs/proto/awzy/authz/v1/authz.proto).

| Request field   |                                                           |
| --------------- | --------------------------------------------------------- |
| `principal`     | Who is acting: `{type: "Human" \| "ServiceAccount", id}`. |
| `action`        | What they're doing, e.g. `read`.                          |
| `resource`      | What they're doing it to.                                 |
| `context`       | Request context; must match the schema for the action.    |
| `on_behalf_of`  | The person a service account acts for.                    |
| `human_present` | Whether a person can answer a challenge now.              |

| Response field  |                                                  |
| --------------- | ------------------------------------------------ |
| `decision_qid`  | The decision's ID, for explaining it later.      |
| `verdict`       | `ALLOW`, `DENY`, or `CHALLENGE`.                 |
| `challenges`    | Every challenge that must be passed.             |
| `policies`      | The policies that decided it.                    |
| `mode`          | `MONITOR` means allow whatever the verdict says. |
| `challenge_qid` | The challenge to answer before retrying.         |

An enforcement point that doesn't understand `mode` applies the verdict, which fails safe.

### Waiting for a challenge's answer

`awzy.authz.v1.Authorizer/WaitForChallenge`, for an agent with no person in its session: it waits
while awzy asks the person, then retries the same request.

| Request field               |                                                                     |
| --------------------------- | ------------------------------------------------------------------- |
| `challenge_qid`             | From the check's response.                                          |
| `principal`, `on_behalf_of` | The check's, as you validated them. Anyone else's gets `NOT_FOUND`. |
| `wait_secs`                 | How long to wait for an answer: 0 to look, at most 25.              |

| Response field     |                                                                                                      |
| ------------------ | ---------------------------------------------------------------------------------------------------- |
| `outcome`          | `PENDING` (ask again), `PASSED` (retry), `REJECTED` (don't), or `EXPIRED` (a retry opens a new one). |
| `expires_at`       | Pending: when it expires unanswered. Rejected: when the request stops being denied.                  |
| `grant_expires_at` | Passed: until when a retry is allowed.                                                               |
| `pending`          | Pending: the challenges still to pass.                                                               |

Over HTTP it's `POST /v1/access/challenge` with the same fields as JSON.

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

## Query plans

When your app lists or searches its own records, ask which ones a principal may act on, and turn
the answer into a filter for your query, instead of checking every row. `POST /v1/access/plan`
(`awzy.authz.v1.Authorizer/Plan` over gRPC) takes the same `principal`, `on_behalf_of`, `action`,
and `context` as an evaluation, and a `resource_type` instead of a resource:

```sh
curl -s http://awzy-pdp:8080/v1/access/plan -H 'content-type: application/json' -d '{
  "principal": {"type": "ServiceAccount", "id": "acme-crm.renewals-bot"},
  "on_behalf_of": {"type": "Human", "id": "dana"},
  "action": "view_customer",
  "resource_type": "Customer",
  "context": {"risk_score": 10}
}'
```

```json
{
  "qid": "plan_0199…",
  "outcome": "conditional",
  "filter": {
    "op": "and",
    "args": [
      { "op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme" },
      { "op": "not", "arg": { "op": "contains", "attr": ["labels"], "value": "legal-hold" } }
    ]
  },
  "challenges": [],
  "challenge_could_widen": false,
  "needs_row_checks": false,
  "attributes": ["labels", "tenant"],
  "policies": ["acme-agents-see-acme"],
  "mode": "enforce",
  "policy_version": 12,
  "principals_version": 3081,
  "max_age_secs": 60,
  "pinned": false
}
```

| `outcome`        | What to do                                                                 |
| ---------------- | -------------------------------------------------------------------------- |
| `always_allowed` | Every row. No filter needed.                                               |
| `always_denied`  | No row. Skip the query.                                                    |
| `challenge`      | Every row needs `challenges` passed first; `filter` applies once they are. |
| `conditional`    | The rows `filter` matches.                                                 |

An agent acting for a person sees only rows both may see. In monitor mode the plan still returns
the real filter: show every row, and record what the filter would have hidden.

### The filter

Each node has an `op`. `attr` is a path into the resource's attributes.

| `op`                                       | Matches a row when                                                                        |
| ------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `true`, `false`                            | Always, never.                                                                            |
| `and`, `or` (`args`), `not` (`arg`)        | As in boolean logic.                                                                      |
| `compare`                                  | `attr` compared by `cmp` (`eq`, `ne`, `lt`, `le`, `gt`, `ge`) with `value`.               |
| `has`                                      | The row has `attr`.                                                                       |
| `contains`, `contains_any`, `contains_all` | The set at `attr` holds `value`, any of `values`, all of `values`.                        |
| `like`                                     | The text at `attr` matches `pattern`, where `*` matches anything.                         |
| `in`                                       | The row is `ancestor` or under it in your hierarchy: join on its ancestor closure.        |
| `is`                                       | The row is the entity `id`.                                                               |
| `unknown`, `unmappable`                    | awzy can't say from the row's attributes. Check these rows one by one with an evaluation. |

`needs_row_checks` is true when the filter has an `unknown` or `unmappable` node. A row with an
attribute missing never matches, so treat a missing value as not matching, as SQL's `NULL` does.
`challenge_could_widen` says a challenge hides some rows, which passing it would show.

### Paging and freshness

A plan is good for `max_age_secs`. To page through results with the same filter, send the first
page's `qid` as `plan_qid` with each next page: you get the same plan back, with `pinned: true`,
until it's older than `max_age_secs` or the principal is contained. A new plan is made sooner when
policies change or awzy rescores the principal.

### Reporting what a plan returned

Send what you showed to awzy-intake as an app event with `access: "listed"`, `count` set to the
number of rows, and the plan's `qid` in the event's `attributes` as `plan_qid`. awzy's risk scores
count listed rows like any other access, so a bulk read through a plan still stands out.
