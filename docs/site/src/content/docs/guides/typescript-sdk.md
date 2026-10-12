---
title: Checking calls in a TypeScript app
description: "Using awzy's TypeScript SDK to check each tool call or operation in your own app."
---

When your own app takes actions for people and agents, such as typed tool functions an agent
calls from a script, check each one with awzy's TypeScript SDK, `@awzy/sdk`. It's an enforcement
point in your app's process, held to the same rules as awzy-gateway:

- It validates the caller's token itself, and takes who's acting from it, never from your code.
- It sends awzy the IDs of what's touched, not their attributes, so awzy decides on its own data.
- In monitor mode it allows every call and records what enforcing would have done.
- A challenge comes back as a typed result you can wait on, never as a plain deny.
- When awzy can't answer in time it falls back as awzy-gateway does.
- Every decision's ID reaches your logs.

## Set it up

```ts
import { Awzy, TokenVerifier, serviceTokenFromDir } from "@awzy/sdk";

const awzy = new Awzy({
  pdpUrl: "http://awzy-pdp.awzy-system.svc:8080",
  // Tokens from your IdP, as awzy-gateway takes them.
  tokens: new TokenVerifier({
    issuer: "https://idp.example.com",
    audience: "awzy",
    keys: "https://idp.example.com/.well-known/jwks.json",
  }),
  // In Kubernetes, the service token the cluster mounts for awzy-pdp.
  serviceToken: serviceTokenFromDir(),
  budgetMs: 250,
  onDecision: (d) => log.info({ decision: d.decisionQid, action: d.action, result: d.result }),
});
```

If your app mints its own agents' tokens, add it to `trustedApps` as an admin trusted it on the
Directory page's Apps tab. Its tokens are then accepted for its own agents only.

## Check each call

`run` checks a call, runs it only if awzy allows it, and throws `Blocked` if not:

```ts
await awzy.run(
  { authorization: req.headers.authorization, action: "update_invoice", resource: { type: "Invoice", id } },
  () => updateInvoice(id, amount),
);
```

Or check first and decide yourself. The result is one of:

| `kind`            | What to do                                                                                                      |
| ----------------- | --------------------------------------------------------------------------------------------------------------- |
| `allowed`         | Go ahead. In monitor mode, `verdict` says what enforcing would have done.                                       |
| `denied`          | Don't. Tell the person, with `decisionQid`.                                                                     |
| `challenged`      | The person must answer `challengeQid` first. `await result.wait()`, and retry the same call when it's `passed`. |
| `unavailable`     | awzy couldn't decide and nothing allows the call without it. Retry shortly.                                     |
| `unauthenticated` | The caller's token isn't valid.                                                                                 |

`checkMany` checks several calls at once, each its own decision, within one budget.

An agent with nobody in its session can wait on a challenge: awzy tells its person, who answers
in the client portal or from Slack, and `wait()` resolves when they do. Waiting hands your app
nothing it could replay; the retry is allowed because awzy has the answer.

## Lists and searches

To show a principal only the records they may act on, ask for a plan and turn it into a filter
for your own query, instead of checking every row. `toPostgres` is a reference mapper for
Postgres; write your own for another store from the same filter tree.

```ts
import { toPostgres } from "@awzy/sdk";

const req = { authorization, action: "view_customer", resourceType: "Customer", planQid: cursor?.planQid };
const plan = await awzy.plan(req);
if (!("ok" in plan)) throw new Error(`awzy couldn't plan this: ${plan.reason}`);

const sql = toPostgres(plan.apply, {
  id: "customers.id",
  columns: { tenant: "tenant", labels: "labels", owner: "owner_id" },
  ancestors: { table: "customer_regions", row: "customer_id", ancestor: "region_id" },
});
const rows = await db.query(`SELECT * FROM customers WHERE ${sql.where} LIMIT 50`, sql.params);

// What the mapper couldn't express, it left open: check those rows one by one.
const shown = sql.needsRowChecks ? await keepAllowed(rows) : rows;
await awzy.reportPlan(req, plan, shown.length);
```

- `plan.apply` is the filter to use now: every row in monitor mode, none while a challenge is
  pending, and otherwise the plan's own. `plan.filter` is the plan's filter whatever the mode.
- Pass the first page's `plan.qid` as `planQid` for the next pages, to page with the same filter.
- `reportPlan` records how many rows you showed, so awzy's risk scores see bulk reads through
  plans. Call it for every page.
- A row missing an attribute the filter reads never matches, as SQL's `NULL` doesn't.

Each check has a budget, 250 ms unless you set `budgetMs`. A check that runs out of it, or can't
reach awzy at all, falls back:

- While the install is in monitor mode, the call is allowed.
- In enforce mode it's refused, unless all of these hold: you listed the action in `failOpen`,
  the resource's `tier` is one of `failOpenTiers` (set them as the client portal's outage fallback
  says), the tier isn't `confidential` or `crown_jewels`, and awzy allowed the same call recently.

## Recording activity

Pass an `ActivityLog` with awzy-intake's URL and token, and every call `run` allows is recorded,
with whether it succeeded, so awzy's risk scores see what agents do through your app.
