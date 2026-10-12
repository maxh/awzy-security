// Command partner-e2e runs the design partner's path through this library against a running
// install (tools/demo partner): an app-minted agent's token validated, a check in monitor, a query
// plan turned into SQL, and the plan's results reported to awzy-intake. Inputs are AWZY_TEST_*
// variables; it exits non-zero on the first thing that isn't as it should be.
package main

import (
	"context"
	"fmt"
	"os"
	"time"

	awzyauthz "github.com/maxh/awzy-security/authz/sdks/go"
)

func env(k string) string {
	v := os.Getenv("AWZY_TEST_" + k)
	if v == "" {
		fail("AWZY_TEST_" + k + " is not set")
	}
	return v
}

func ok(what string) { fmt.Println("ok   go: " + what) }

func fail(what string) {
	fmt.Println("FAIL go: " + what)
	os.Exit(1)
}

// resources is awzy's Resource as an app that syncs its labels might store it.
var resources = awzyauthz.SQLMapping{
	ID: "documents.id",
	Columns: map[string]string{
		"sensitivity": "sensitivity", "unclassified": "unclassified", "owner": "owner_id",
		"shared_humans": "shared_human_ids", "shared_groups": "shared_group_ids",
		"writer_humans": "writer_human_ids", "writer_groups": "writer_group_ids",
		"shared_with_everyone": "shared_with_everyone",
	},
	Ancestors: &awzyauthz.AncestorTable{Table: "document_ancestors", Row: "document_id", Ancestor: "ancestor_id"},
}

func main() {
	ctx := context.Background()
	ca := env("CA_FILE")
	idpKeys, err := awzyauthz.JWKS(ctx, env("IDP_JWKS_URL"), ca)
	if err != nil {
		fail("the IdP's keys: " + err.Error())
	}
	appKeys, err := awzyauthz.JWKS(ctx, env("APP_JWKS_URL"), ca)
	if err != nil {
		fail("the app's keys: " + err.Error())
	}
	tokens := awzyauthz.NewTokenVerifier(awzyauthz.TokenOptions{
		Issuer: env("IDP_ISSUER"), Audience: "awzy", Keys: idpKeys,
		TrustedApps: []awzyauthz.TrustedApp{{Name: env("APP_NAME"), Issuer: env("APP_ISSUER"), Audience: env("APP_AUDIENCE"), Keys: appKeys}},
	})
	activity := awzyauthz.NewActivityLog(env("INTAKE_URL"), env("INTAKE_TOKEN"), env("SYSTEM"))
	awzy := awzyauthz.New(awzyauthz.Options{
		PDPURL: env("PDP_URL"), Tokens: tokens, ServiceToken: awzyauthz.ServiceTokenFromDir(env("SERVICE_TOKEN_DIR")),
		Budget: 2 * time.Second, Activity: activity,
	})
	authorization := "Bearer " + env("AGENT_TOKEN")

	// 1. The app-minted agent's token: the agent, acting for its person.
	p, err := tokens.Principal(authorization)
	if err != nil || p.Principal.ID != env("AGENT") || p.OnBehalfOf == nil || p.OnBehalfOf.ID != env("HUMAN") {
		fail(fmt.Sprintf("token: %+v %v", p, err))
	}
	ok(fmt.Sprintf("the app's token is %s acting for %s", p.Principal.ID, p.OnBehalfOf.ID))

	// 2. A check, which the install's monitor mode allows whatever the verdict.
	r := awzy.Check(ctx, awzyauthz.CheckRequest{Action: "read", Resource: awzyauthz.Resource{Type: "Resource", ID: "demo-check"}, Authorization: authorization})
	if !r.Allowed || r.Mode != "monitor" || r.DecisionQID == "" {
		fail(fmt.Sprintf("check: %+v", r))
	}
	ok(fmt.Sprintf("a check is decided (%s, %s, %s)", r.Verdict, r.Mode, r.DecisionQID))

	// 3. A plan for what the agent may read, its filter mapped to SQL.
	req := awzyauthz.PlanRequest{Action: "read", ResourceType: "Resource", Authorization: authorization}
	plan, err := awzy.Plan(ctx, req)
	if err != nil || plan.QID == "" {
		fail(fmt.Sprintf("plan: %+v %v", plan, err))
	}
	sql := awzyauthz.ToPostgres(plan.Filter, resources, 1)
	where := sql.Where
	if len(where) > 80 {
		where = where[:80] + "…"
	}
	ok(fmt.Sprintf("a plan (%s) maps to WHERE %s", plan.Outcome, where))

	// 4. Its results reported to awzy-intake as a listed access linked to the plan.
	if err := awzy.ReportPlan(req, plan, 3); err != nil {
		fail("report: " + err.Error())
	}
	activity.Flush(ctx)
	ok("the plan's results are reported to awzy-intake (" + plan.QID + ")")
}
