// The conformance spec every awzy client library runs (ADR 0114): authz/sdks/conformance.

import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { Awzy } from "./client.ts";
import type { CheckResult, Principal } from "./types.ts";

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);

const spec: unknown = JSON.parse(readFileSync(new URL("../../conformance/scenarios.json", import.meta.url), "utf8"));
if (!isRecord(spec)) throw new Error("the conformance spec isn't an object");

function entity(v: unknown): { type: string; id: string } {
  if (!isRecord(v) || typeof v["type"] !== "string" || typeof v["id"] !== "string") {
    throw new Error(`not an entity: ${JSON.stringify(v)}`);
  }
  return { type: v["type"], id: v["id"] };
}

const principal: Principal = { principal: entity(spec["principal"]), onBehalfOf: entity(spec["on_behalf_of"]) };

/** What the fake PDP answers: a response, a refused connection (`down`), or nothing (`hang`). */
function answer(a: unknown, signal: AbortSignal | null | undefined): Promise<Response> {
  if (a === "down") return Promise.reject(new TypeError("fetch failed"));
  if (a === "hang") {
    return new Promise((_, reject) => signal?.addEventListener("abort", () => reject(new Error("aborted"))));
  }
  if (!isRecord(a)) return Promise.reject(new Error(`no answer set: ${JSON.stringify(a)}`));
  const status = typeof a["status"] === "number" ? a["status"] : 200;
  return Promise.resolve(new Response(JSON.stringify(a["body"] ?? null), { status }));
}

/** A fetch answering as the current step says, recording the evaluation bodies it was sent. */
function fakePdp() {
  let step: Record<string, unknown> = {};
  const sent: unknown[] = [];
  const fetch: typeof globalThis.fetch = (url, init) => {
    const href = typeof url === "string" ? url : url instanceof URL ? url.href : url.url;
    const path = new URL(href).pathname;
    if (path === "/v1/access/evaluation") {
      sent.push(JSON.parse(typeof init?.body === "string" ? init.body : "null"));
      return answer(step["evaluation"], init?.signal);
    }
    return answer(step["challenge"] ?? "down", init?.signal);
  };
  return { fetch, sent, set: (s: Record<string, unknown>) => (step = s) };
}

/** The result's fields under the spec's names. */
function fields(r: CheckResult): Record<string, unknown> {
  const out: Record<string, unknown> = { allowed: r.allowed, kind: r.kind };
  if (r.kind === "allowed") {
    Object.assign(out, { mode: r.mode, verdict: r.verdict, fallback: r.fallback, decision_qid: r.decisionQid });
  } else if (r.kind === "denied") {
    Object.assign(out, { decision_qid: r.decisionQid, policies: r.policies });
  } else if (r.kind === "challenged") {
    Object.assign(out, {
      decision_qid: r.decisionQid,
      challenge_qid: r.challengeQid,
      challenges: r.challenges,
      expires_at: r.expiresAt,
    });
  }
  return out;
}

const scenarios = Array.isArray(spec["scenarios"]) ? spec["scenarios"].filter(isRecord) : [];

describe("conformance", () => {
  it("has scenarios", () => expect(scenarios.length).toBeGreaterThan(10));

  for (const scenario of scenarios) {
    it(String(scenario["name"]), async () => {
      const options = isRecord(scenario["options"]) ? scenario["options"] : {};
      const pdp = fakePdp();
      const awzy = new Awzy({
        pdpUrl: "http://pdp",
        fetch: pdp.fetch,
        ...(typeof options["budget_ms"] === "number" ? { budgetMs: options["budget_ms"] } : {}),
        failOpen: Object.fromEntries(strings(options["fail_open"]).map((a) => [a, true])),
        failOpenTiers: strings(options["fail_open_tiers"]),
      });
      const steps = Array.isArray(scenario["steps"]) ? scenario["steps"].filter(isRecord) : [];
      for (const [i, step] of steps.entries()) {
        pdp.set(step);
        const request = isRecord(step["request"]) ? step["request"] : {};
        const resource = isRecord(request["resource"]) ? request["resource"] : {};
        const before = pdp.sent.length;
        // Steps run in order: each one's answer depends on what the client saw before.
        // oxlint-disable-next-line no-await-in-loop
        const result = await awzy.check({
          principal: typeof scenario["work_item_id"] === "string" ? { ...principal, workItemId: scenario["work_item_id"] } : principal,
          action: String(request["action"]),
          resource: {
            ...entity(resource),
            ...(typeof resource["tier"] === "string" ? { tier: resource["tier"] } : {}),
          },
        });
        const got = fields(result);
        const expected = isRecord(step["expect"]) ? step["expect"] : {};
        for (const [key, value] of Object.entries(expected)) {
          expect(got[key], `step ${i}: ${key}`).toEqual(value);
        }
        if (step["sent"] !== undefined) {
          expect(pdp.sent[before], `step ${i}: sent`).toEqual(step["sent"]);
        }
      }
    });
  }
});
