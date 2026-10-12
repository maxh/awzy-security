---
title: Python
description: "Checking each tool call or operation in a Python app or agent with awzy-authz-sdk."
---

`awzy-authz-sdk` checks each action your Python app or agent takes with awzy, and behaves exactly
as every awzy client library does: see [Client libraries](/client-libraries/).

```sh
pip install awzy-authz-sdk
```

## Set it up

```python
from awzy_authz import Awzy, TokenVerifier, service_token_from_dir

awzy = Awzy(
    "http://awzy-pdp.awzy-system.svc:8080",
    # Tokens from your IdP, as awzy-gateway takes them.
    tokens=TokenVerifier("https://idp.example.com", "awzy", "https://idp.example.com/.well-known/jwks.json"),
    # In Kubernetes, the service token the cluster mounts for awzy-pdp.
    service_token=service_token_from_dir(),
    budget_ms=250,
    on_decision=lambda d: log.info("awzy", extra={"decision": d.decision_qid, "action": d.action, "result": d.result}),
)
```

If your app mints its own agents' tokens, pass it in `trusted_apps` as an admin trusted it on the
Directory page's Apps tab. Its tokens are then accepted for its own agents only.

## Check each call

`run` checks a call, runs it only if awzy allows it, and raises `Blocked` if not:

```python
from awzy_authz import CheckRequest, Resource

awzy.run(
    CheckRequest("update_invoice", Resource("Invoice", invoice_id), authorization=request.headers["authorization"]),
    lambda: update_invoice(invoice_id, amount),
)
```

Or check first with `awzy.check(...)` and decide yourself. The result is `Allowed`, `Denied`,
`Challenged`, `Unavailable`, or `Unauthenticated`, each with `allowed` and `kind`. A `Challenged`
result has `challenge_qid` and `expires_at`; `result.wait()` returns when the person answers, and
on `passed` you retry the same call. `check_many` checks several calls at once, each its own
decision, within one budget.

## Lists and searches

```python
from awzy_authz import SqlMapping, to_postgres

plan = awzy.plan("view_customer", "Customer", authorization=auth, plan_qid=cursor.plan_qid if cursor else None)
sql = to_postgres(plan.apply, SqlMapping(
    id="customers.id",
    columns={"tenant": "tenant", "labels": "labels", "owner": "owner_id"},
    ancestors=("customer_regions", "customer_id", "region_id"),
))
rows = db.execute(f"SELECT * FROM customers WHERE {sql.where} LIMIT 50", sql.params).fetchall()
shown = keep_allowed(rows) if sql.needs_row_checks else rows
awzy.report_plan(plan, "view_customer", "Customer", len(shown), authorization=auth)
```

`plan.apply` is every row in monitor mode, none while a challenge is pending, and otherwise the
plan's own filter. What the mapper can't express it leaves open and reports, so you check just
those rows one by one.

## When awzy can't be reached

Each check has a budget, 250 ms unless you set `budget_ms`. One that runs out of it, or can't
reach awzy, is allowed while the install monitors, and otherwise refused, unless you listed the
action in `fail_open`, the resource's `tier` is in `fail_open_tiers` (as the client portal's outage
fallback says) and isn't `confidential` or `crown_jewels`, and awzy allowed the same call recently.

## Recording activity

Pass an `ActivityLog` with awzy-intake's URL and token, and every call `run` allows is recorded with
whether it succeeded. Call `activity.flush()` before a script exits.
