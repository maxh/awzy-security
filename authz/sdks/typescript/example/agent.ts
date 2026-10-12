// A sample agent script: each typed tool function it calls is checked with awzy first, as the
// agent acting for its person, on the invoice it touches. Run against an install (or tools/demo):
//
//   AWZY_PDP_URL=http://localhost:8082 AWZY_AUTH_ISSUER=… AWZY_AUTH_JWKS_URL=… \
//   AGENT_TOKEN="Bearer …" node example/agent.ts
//
// In monitor mode every call runs and the decisions show what enforcing would do; in enforce mode
// a denied call doesn't run, and a challenged one waits for the person, then retries.

import { Awzy, Blocked, TokenVerifier } from "../src/index.ts";

const env = (k: string, fallback?: string): string => {
  const v = process.env[k] ?? fallback;
  if (v === undefined) throw new Error(`${k} is not set`);
  return v;
};

const awzy = new Awzy({
  pdpUrl: env("AWZY_PDP_URL", "http://localhost:8082"),
  tokens: new TokenVerifier({
    issuer: env("AWZY_AUTH_ISSUER"),
    audience: env("AWZY_AUTH_AUDIENCE", "awzy"),
    keys: env("AWZY_AUTH_JWKS_URL"),
  }),
  failOpen: { read_invoice: true },
  failOpenTiers: ["internal"],
  onDecision: (d) => console.log(`${d.action} ${d.resource.id}: ${d.result} (${d.decisionQid ?? "no decision"})`),
});

const authorization = env("AGENT_TOKEN");

// The app's own tool functions. Nothing here knows about awzy.
const invoices = new Map([["inv-7", { id: "inv-7", amount: 120 }]]);
const readInvoice = (id: string) => invoices.get(id);
const updateInvoice = (id: string, amount: number) => invoices.set(id, { id, amount });

/** Runs a tool call through awzy, waiting for a challenge's answer and retrying once it's passed. */
async function tool<T>(action: string, id: string, call: () => T): Promise<T | undefined> {
  const req = { authorization, action, resource: { type: "Invoice", id, tier: "internal" } };
  try {
    return await awzy.run(req, call);
  } catch (e) {
    if (!(e instanceof Blocked)) throw e;
    const r = e.result;
    if (r.kind !== "challenged") {
      console.log(`not done: ${e.message}`);
      return undefined;
    }
    console.log(`waiting for ${r.challenges.join(" and ")} on ${r.challengeQid} (until ${r.expiresAt ?? "?"})`);
    const answer = await r.wait();
    if (answer.outcome !== "passed") {
      console.log(`challenge ${answer.outcome}`);
      return undefined;
    }
    // Passed: the same call again, which awzy now allows.
    return tool(action, id, call);
  }
}

console.log(await tool("read_invoice", "inv-7", () => readInvoice("inv-7")));
await tool("update_invoice", "inv-7", () => updateInvoice("inv-7", 150));
