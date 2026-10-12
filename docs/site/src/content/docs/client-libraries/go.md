---
title: Go
description: "Checking each operation in a Go service or agent with awzy's Go client library."
---

The `awzyauthz` package checks each action your Go service or agent takes with awzy, and behaves
exactly as every awzy client library does: see [Client libraries](/client-libraries/).

```sh
go get github.com/maxh/awzy-security/authz/sdks/go
```

## Set it up

```go
import awzyauthz "github.com/maxh/awzy-security/authz/sdks/go"

keys, err := awzyauthz.JWKS(ctx, "https://idp.example.com/.well-known/jwks.json", "")
if err != nil {
	return err
}
awzy := awzyauthz.New(awzyauthz.Options{
	PDPURL:       "http://awzy-pdp.awzy-system.svc:8080",
	Tokens:       awzyauthz.NewTokenVerifier(awzyauthz.TokenOptions{Issuer: "https://idp.example.com", Audience: "awzy", Keys: keys}),
	ServiceToken: awzyauthz.ServiceTokenFromDir(""), // in Kubernetes, the token mounted for awzy-pdp
	Budget:       250 * time.Millisecond,
	OnDecision:   func(d awzyauthz.DecisionNote) { slog.Info("awzy", "decision", d.DecisionQID, "action", d.Action, "result", d.Result) },
})
```

If your app mints its own agents' tokens, add it to `TrustedApps` as an admin trusted it on the
Directory page's Apps tab. Its tokens are then accepted for its own agents only.

## Check each call

`Run` checks a call, runs it only if awzy allows it, and returns a `*Blocked` error if not:

```go
invoice, err := awzyauthz.Run(ctx, awzy, awzyauthz.CheckRequest{
	Action:        "update_invoice",
	Resource:      awzyauthz.Resource{Type: "Invoice", ID: id, Tier: "internal"},
	Authorization: r.Header.Get("Authorization"),
}, func() (Invoice, error) { return updateInvoice(id, amount) })
```

Or call `awzy.Check(ctx, req)` and decide yourself. The result has `Allowed` and `Kind`
(`allowed`, `denied`, `challenged`, `unavailable`, `unauthenticated`). For a challenge,
`result.Wait(ctx)` returns when the person answers; on `passed`, retry the same call. `CheckMany`
checks several calls at once within one budget.

## Lists and searches

```go
req := awzyauthz.PlanRequest{Action: "view_customer", ResourceType: "Customer", Authorization: auth, PlanQID: cursor.PlanQID}
plan, err := awzy.Plan(ctx, req)
if err != nil {
	return err // awzy can't answer: show nothing, or check rows one by one
}
sql := awzyauthz.ToPostgres(plan.Apply, awzyauthz.SQLMapping{
	ID:        "customers.id",
	Columns:   map[string]string{"tenant": "tenant", "labels": "labels", "owner": "owner_id"},
	Ancestors: &awzyauthz.AncestorTable{Table: "customer_regions", Row: "customer_id", Ancestor: "region_id"},
}, 1)
rows, err := db.Query(ctx, "SELECT * FROM customers WHERE "+sql.Where+" LIMIT 50", sql.Params...)
// If sql.NeedsRowChecks, check each row one by one before showing it.
_ = awzy.ReportPlan(req, plan, shown)
```

## When awzy can't be reached

Each check has a budget, 250 ms unless you set `Budget`. One that runs out of it, or can't reach
awzy, is allowed while the install monitors, and otherwise refused, unless you listed the action
in `FailOpen`, the resource's `Tier` is in `FailOpenTiers` (as the client portal's outage fallback
says) and isn't `confidential` or `crown_jewels`, and awzy allowed the same call recently.

## Recording activity

Pass an `ActivityLog` from `NewActivityLog(intakeURL, token, "your-app")`, and every call `Run`
allows is recorded with whether it succeeded. Call `Flush` before the program exits.
