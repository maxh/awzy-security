// The design partner's path through this library, against a running install (tools/demo partner):
// an app-minted agent's token validated, a check in monitor, a query plan turned into SQL, and the
// plan's results reported to awzy-intake. Inputs are AWZY_TEST_* variables; it exits non-zero on
// the first thing that isn't as it should be.

import { ActivityLog, Awzy, serviceTokenFromDir, type SqlMapping, TokenVerifier, toPostgres } from "../src/index.ts";

const env = (k: string): string => {
  const v = process.env[`AWZY_TEST_${k}`];
  if (!v) throw new Error(`AWZY_TEST_${k} is not set`);
  return v;
};

function ok(what: string): void {
  console.log(`ok   typescript: ${what}`);
}

function fail(what: string): never {
  console.log(`FAIL typescript: ${what}`);
  process.exit(1);
}

/** awzy's Resource as an app that syncs its labels might store it. */
const resources: SqlMapping = {
  id: "documents.id",
  columns: {
    sensitivity: "sensitivity",
    unclassified: "unclassified",
    owner: "owner_id",
    shared_humans: "shared_human_ids",
    shared_groups: "shared_group_ids",
    writer_humans: "writer_human_ids",
    writer_groups: "writer_group_ids",
    shared_with_everyone: "shared_with_everyone",
  },
  ancestors: { table: "document_ancestors", row: "document_id", ancestor: "ancestor_id" },
};

const tokens = new TokenVerifier({
  issuer: env("IDP_ISSUER"),
  audience: "awzy",
  keys: env("IDP_JWKS_URL"),
  trustedApps: [
    { name: env("APP_NAME"), issuer: env("APP_ISSUER"), keys: env("APP_JWKS_URL"), audience: env("APP_AUDIENCE") },
  ],
});
const activity = new ActivityLog({ url: env("INTAKE_URL"), token: env("INTAKE_TOKEN"), system: env("SYSTEM") });
const awzy = new Awzy({
  pdpUrl: env("PDP_URL"),
  tokens,
  serviceToken: serviceTokenFromDir(env("SERVICE_TOKEN_DIR")),
  budgetMs: 2000,
  activity,
});
const authorization = `Bearer ${env("AGENT_TOKEN")}`;

// 1. The app-minted agent's token: the agent, acting for its person.
const p = await tokens.principal(authorization).catch((e: unknown) => fail(`token: ${String(e)}`));
if (p.principal.id !== env("AGENT") || p.onBehalfOf?.id !== env("HUMAN")) {
  fail(`token named ${JSON.stringify(p)}`);
}
ok(`the app's token is ${p.principal.id} acting for ${p.onBehalfOf.id}`);

// 2. A check, which the install's monitor mode allows whatever the verdict.
const result = await awzy.check({ authorization, action: "read", resource: { type: "Resource", id: "demo-check" } });
if (result.kind !== "allowed" || result.mode !== "monitor" || !result.decisionQid) {
  fail(`check: ${JSON.stringify(result)}`);
}
ok(`a check is decided (${result.verdict}, ${result.mode}, ${result.decisionQid})`);

// 3. A plan for what the agent may read, its filter mapped to SQL.
const req = { authorization, action: "read", resourceType: "Resource" };
const plan = await awzy.plan(req);
if (!("ok" in plan) || !plan.qid) fail(`plan: ${JSON.stringify(plan)}`);
const sql = toPostgres(plan.filter, resources);
ok(`a plan (${plan.outcome}) maps to WHERE ${sql.where.slice(0, 80)}${sql.where.length > 80 ? "…" : ""}`);

// 4. Its results reported to awzy-intake as a listed access linked to the plan.
await awzy.reportPlan(req, plan, 3);
await activity.flush();
ok(`the plan's results are reported to awzy-intake (${plan.qid})`);
