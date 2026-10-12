// The conformance scenarios (ADR 0045, rule 7): what every enforcement point does with each answer
// the PDP can give, against a fake PDP that answers as awzy-pdp's JSON API does.

import { describe, expect, it, vi } from "vitest";

import { ActivityLog } from "./activity.ts";
import { Awzy, Blocked } from "./client.ts";
import type { CheckRequest, DecisionNote } from "./types.ts";

type Handler = (path: string, body: Record<string, unknown>) => { status?: number; body?: unknown } | "hang" | "down";

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** A fetch that answers as `handler` says, and records every request. */
function fakePdp(handler: Handler) {
  const calls: { path: string; body: Record<string, unknown>; authorization?: string }[] = [];
  const fetch: typeof globalThis.fetch = async (url, init) => {
    const href = typeof url === "string" ? url : url instanceof URL ? url.href : url.url;
    const path = new URL(href).pathname;
    const parsed: unknown = JSON.parse(typeof init?.body === "string" ? init.body : "{}");
    const body = isRecord(parsed) ? parsed : {};
    const authorization = new Headers(init?.headers).get("authorization");
    calls.push({ path, body, ...(authorization ? { authorization } : {}) });
    const answer = handler(path, body);
    if (answer === "down") throw new TypeError("fetch failed");
    if (answer === "hang") {
      return new Promise<Response>((_, reject) => {
        init?.signal?.addEventListener("abort", () => reject(new Error("aborted")));
      });
    }
    return new Response(JSON.stringify(answer.body ?? {}), { status: answer.status ?? 200 });
  };
  return { fetch, calls };
}

const bot = { principal: { type: "ServiceAccount", id: "renewals-bot" }, onBehalfOf: { type: "Human", id: "dana" } };

function req(extra: Partial<CheckRequest> = {}): CheckRequest {
  return {
    principal: bot,
    action: "update_invoice",
    resource: { type: "Invoice", id: "inv-7", tier: "internal" },
    ...extra,
  };
}

function decision(verdict: string, mode: string, extra: Record<string, unknown> = {}) {
  return { body: { qid: "decision_0192a000-0000-7000-8000-000000000001", verdict, policies: ["p"], mode, ...extra } };
}

describe("Awzy.check", () => {
  it("sends IDs, the person, and its service token, and allows what the PDP allows", async () => {
    const pdp = fakePdp(() => decision("allow", "enforce"));
    const notes: DecisionNote[] = [];
    const awzy = new Awzy({
      pdpUrl: "http://pdp",
      fetch: pdp.fetch,
      serviceToken: () => "svc",
      onDecision: (n) => notes.push(n),
    });
    const r = await awzy.check(req());
    expect(r).toMatchObject({ allowed: true, kind: "allowed", mode: "enforce", verdict: "allow" });
    expect(pdp.calls[0]).toEqual({
      path: "/v1/access/evaluation",
      authorization: "Bearer svc",
      body: {
        principal: bot.principal,
        on_behalf_of: bot.onBehalfOf,
        action: "update_invoice",
        resource: { type: "Invoice", id: "inv-7" },
        context: {},
        entities: [],
      },
    });
    expect(notes[0]).toMatchObject({ decisionQid: "decision_0192a000-0000-7000-8000-000000000001", result: "allowed" });
  });

  it("allows in monitor mode whatever the verdict, and says what enforcing would have done", async () => {
    const awzy = new Awzy({ pdpUrl: "http://pdp", fetch: fakePdp(() => decision("deny", "monitor")).fetch });
    expect(await awzy.check(req())).toMatchObject({ allowed: true, mode: "monitor", verdict: "deny" });
  });

  it("denies in enforce mode, and reads an unknown verdict or mode as the safe one", async () => {
    const deny = new Awzy({ pdpUrl: "http://pdp", fetch: fakePdp(() => decision("deny", "enforce")).fetch });
    expect(await deny.check(req())).toMatchObject({ allowed: false, kind: "denied", policies: ["p"] });
    const odd = new Awzy({ pdpUrl: "http://pdp", fetch: fakePdp(() => decision("maybe", "later")).fetch });
    expect(await odd.check(req())).toMatchObject({ allowed: false, kind: "denied" });
  });

  it("surfaces a challenge with its ID and expiry, and waits for the person's answer", async () => {
    let asked = 0;
    const pdp = fakePdp((path) => {
      if (path === "/v1/access/evaluation") {
        return decision("challenge", "enforce", {
          challenges: ["approval"],
          challenge_qid: "challenge_0192a000-0000-7000-8000-000000000002",
        });
      }
      asked += 1;
      return asked < 3
        ? { body: { outcome: "pending", expires_at: "2026-10-12T15:00:00Z", pending: ["approval"] } }
        : { body: { outcome: "passed", grant_expires_at: "2026-10-12T15:05:00Z" } };
    });
    const awzy = new Awzy({ pdpUrl: "http://pdp", fetch: pdp.fetch });
    const r = await awzy.check(req());
    if (r.kind !== "challenged") throw new Error(`expected a challenge, got ${r.kind}`);
    expect(r).toMatchObject({
      challengeQid: "challenge_0192a000-0000-7000-8000-000000000002",
      challenges: ["approval"],
      expiresAt: "2026-10-12T15:00:00Z",
    });
    expect(await r.wait()).toEqual({ outcome: "passed", grantExpiresAt: "2026-10-12T15:05:00Z" });
    expect(pdp.calls.at(-1)?.body).toMatchObject({
      principal: bot.principal,
      on_behalf_of: bot.onBehalfOf,
      wait_secs: 25,
    });
  });

  it("is unavailable when a challenge couldn't be opened", async () => {
    const awzy = new Awzy({ pdpUrl: "http://pdp", fetch: fakePdp(() => decision("challenge", "enforce")).fetch });
    expect(await awzy.check(req())).toMatchObject({ allowed: false, kind: "unavailable" });
  });

  it("refuses a caller whose token isn't valid without asking the PDP", async () => {
    const pdp = fakePdp(() => decision("allow", "enforce"));
    const awzy = new Awzy({ pdpUrl: "http://pdp", fetch: pdp.fetch });
    expect(
      await awzy.check({ action: "read", resource: { type: "Invoice", id: "1" }, authorization: "Bearer x" }),
    ).toMatchObject({
      allowed: false,
      kind: "unauthenticated",
    });
    expect(pdp.calls).toHaveLength(0);
  });
});

describe("the outage fallback", () => {
  it("keeps allowing through an outage when the install was monitoring", async () => {
    let down = false;
    const awzy = new Awzy({
      pdpUrl: "http://pdp",
      fetch: fakePdp(() => (down ? "down" : decision("deny", "monitor"))).fetch,
    });
    await awzy.check(req());
    down = true;
    expect(await awzy.check(req())).toMatchObject({ allowed: true, fallback: "monitor" });
  });

  it("fails closed in enforce mode, past the budget too", async () => {
    let answer: "allow" | "hang" | "down" = "allow";
    const awzy = new Awzy({
      pdpUrl: "http://pdp",
      budgetMs: 20,
      fetch: fakePdp(() => (answer === "allow" ? decision("allow", "enforce") : answer)).fetch,
    });
    await awzy.check(req());
    answer = "hang";
    expect(await awzy.check(req())).toMatchObject({ allowed: false, kind: "unavailable", reason: /budget/ });
    answer = "down";
    expect(await awzy.check(req())).toMatchObject({ allowed: false, kind: "unavailable" });
  });

  it("replays a recent allow for a fail-open action on a fail-open tier, and never for crown jewels", async () => {
    let down = false;
    const awzy = new Awzy({
      pdpUrl: "http://pdp",
      failOpen: { update_invoice: true },
      failOpenTiers: ["internal", "crown_jewels"],
      fetch: fakePdp(() => (down ? "down" : decision("allow", "enforce"))).fetch,
    });
    const jewels = req({ resource: { type: "Invoice", id: "ledger", tier: "crown_jewels" } });
    await awzy.check(req());
    await awzy.check(jewels);
    down = true;
    expect(await awzy.check(req())).toMatchObject({ allowed: true, fallback: "cached_allow" });
    expect(await awzy.check(jewels)).toMatchObject({ allowed: false, kind: "unavailable" });
    expect(await awzy.check(req({ resource: { type: "Invoice", id: "inv-8", tier: "internal" } }))).toMatchObject({
      allowed: false,
    });
    expect(await awzy.check(req({ action: "delete_invoice" }))).toMatchObject({ allowed: false });
  });
});

describe("checkMany and run", () => {
  it("decides each call on its own", async () => {
    const pdp = fakePdp((_, body) => decision(body["action"] === "read" ? "allow" : "deny", "enforce"));
    const awzy = new Awzy({ pdpUrl: "http://pdp", fetch: pdp.fetch });
    const results = await awzy.checkMany([req({ action: "read" }), req({ action: "delete" })]);
    expect(results.map((r) => r.allowed)).toEqual([true, false]);
  });

  it("runs an allowed call and records it, and throws Blocked without running a denied one", async () => {
    const intake = fakePdp(() => ({ body: {} }));
    const activity = new ActivityLog({ url: "http://intake", token: "t", system: "acme-crm", fetch: intake.fetch });
    let verdict = "allow";
    const awzy = new Awzy({
      pdpUrl: "http://pdp",
      fetch: fakePdp(() => decision(verdict, "enforce")).fetch,
      activity,
    });
    expect(await awzy.run(req(), () => 42)).toBe(42);
    verdict = "deny";
    const ran = vi.fn();
    await expect(awzy.run(req(), ran)).rejects.toBeInstanceOf(Blocked);
    expect(ran).not.toHaveBeenCalled();
    await activity.flush();
    expect(intake.calls[0]?.body).toMatchObject({
      events: [
        {
          system: "acme-crm",
          principal: bot.principal,
          on_behalf_of: bot.onBehalfOf,
          action: "update_invoice",
          outcome: "success",
        },
      ],
    });
  });
});
