import type { ActivityLog } from "./activity.ts";
import { TokenError, type TokenVerifier } from "./token.ts";
import {
  ALWAYS_FAIL_CLOSED,
  type ChallengeAnswer,
  type ChallengeKind,
  type Challenged,
  type CheckRequest,
  type CheckResult,
  type DecisionNote,
  type Mode,
  type Principal,
  type Verdict,
} from "./types.ts";

export interface AwzyOptions {
  /** awzy-pdp's base URL, e.g. `http://awzy-pdp.awzy-system.svc:8080`. */
  pdpUrl: string;
  /** Validates callers' tokens. Needed for checks that pass `authorization`. */
  tokens?: TokenVerifier;
  /** A service token for awzy-pdp, read fresh for each call (see {@link serviceTokenFromDir}). */
  serviceToken?: () => string | undefined | Promise<string | undefined>;
  /** How long one check may take before the outage fallback decides it. Default 250 ms. */
  budgetMs?: number;
  /** Actions that may fail open while awzy-pdp can't be reached; every other fails closed. */
  failOpen?: Record<string, boolean>;
  /** The fail-open tiers set in the client portal: the ceiling on `failOpen`. Default none. */
  failOpenTiers?: string[];
  /** How long an allow can be replayed while awzy-pdp can't be reached. Default 5 minutes. */
  cachedAllowTtlMs?: number;
  /** Where allowed calls are recorded, when set. */
  activity?: ActivityLog;
  /** Each decision, for the app's logs and traces. */
  onDecision?: (note: DecisionNote) => void;
  fetch?: typeof fetch;
}

/** Thrown by {@link Awzy.run} when a call isn't allowed; `result` says why and what to do. */
export class Blocked extends Error {
  override name = "Blocked";
  readonly result: Exclude<CheckResult, { allowed: true }>;

  constructor(result: Exclude<CheckResult, { allowed: true }>) {
    super(
      result.kind === "denied"
        ? `awzy denied this (decision ${result.decisionQid})`
        : result.kind === "challenged"
          ? `awzy requires ${result.challenges.join(" and ")} (challenge ${result.challengeQid})`
          : `awzy couldn't allow this: ${result.reason}`,
    );
    this.result = result;
  }
}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const text = (r: Record<string, unknown>, key: string): string | undefined => {
  const v = r[key];
  return typeof v === "string" ? v : undefined;
};
const KINDS: ReadonlySet<string> = new Set(["justification", "verification", "approval", "biometric"]);
const isKind = (v: unknown): v is ChallengeKind => typeof v === "string" && KINDS.has(v);
const kinds = (r: Record<string, unknown>, key: string): ChallengeKind[] => {
  const v = r[key];
  return Array.isArray(v) ? v.filter(isKind) : [];
};
const strings = (r: Record<string, unknown>, key: string): string[] => {
  const v = r[key];
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
};

const LONGEST_WAIT_SECS = 25;

/** Reads `<dir>/awzy-pdp`, the service token Kubernetes mounts for the PDP (ADR 0055). */
export function serviceTokenFromDir(
  dir = process.env["AWZY_SERVICE_TOKEN_DIR"] ?? "/var/run/secrets/awzy/tokens",
): () => Promise<string> {
  return async () => {
    const { readFile } = await import("node:fs/promises");
    return (await readFile(`${dir}/awzy-pdp`, "utf8")).trim();
  };
}

/**
 * An enforcement point in the app (ADR 0045): validates the caller, asks awzy-pdp within a
 * budget, applies the mode, surfaces challenges, and falls back as awzy-gateway does.
 */
export class Awzy {
  readonly #options: AwzyOptions;
  readonly #fetch: typeof fetch;
  /** The last mode awzy-pdp reported: a monitoring install keeps allowing through an outage. */
  #lastMode: Mode | undefined;
  /** When awzy-pdp last allowed each request, for replaying during an outage. */
  readonly #allows = new Map<string, number>();

  constructor(options: AwzyOptions) {
    this.#options = options;
    this.#fetch = options.fetch ?? fetch;
  }

  /** Checks one call. Never throws for awzy's answer: the result says what to do. */
  async check(req: CheckRequest): Promise<CheckResult> {
    return this.#check(req, AbortSignal.timeout(this.#options.budgetMs ?? 250));
  }

  /** Checks several calls at once, each its own decision, within one shared budget. */
  async checkMany(reqs: CheckRequest[]): Promise<CheckResult[]> {
    const budget = AbortSignal.timeout(this.#options.budgetMs ?? 250);
    return Promise.all(reqs.map((r) => this.#check(r, budget)));
  }

  /**
   * Runs `call` if awzy allows it, and records it as activity with its outcome. Throws
   * {@link Blocked} otherwise, without running it.
   */
  async run<T>(req: CheckRequest, call: () => Promise<T> | T): Promise<T> {
    let principal: Principal;
    try {
      principal = await this.#principal(req);
    } catch (e) {
      throw new Blocked({
        allowed: false,
        kind: "unauthenticated",
        reason: e instanceof Error ? e.message : String(e),
      });
    }
    const result = await this.check({ ...req, principal });
    if (!result.allowed) throw new Blocked(result);
    const record = (outcome: "success" | "failure") =>
      this.#options.activity?.record({
        principal: principal.principal,
        ...(principal.onBehalfOf ? { onBehalfOf: principal.onBehalfOf } : {}),
        action: req.action,
        resource: { type: req.resource.type, id: req.resource.id },
        outcome,
        attributes: result.decisionQid ? { decision_qid: result.decisionQid } : {},
      });
    try {
      const value = await call();
      record("success");
      return value;
    } catch (e) {
      record("failure");
      throw e;
    }
  }

  async #principal(req: CheckRequest): Promise<Principal> {
    if (req.principal) return req.principal;
    if (!this.#options.tokens) throw new TokenError("no token verifier is configured");
    return this.#options.tokens.principal(req.authorization);
  }

  /** The JSON object awzy-pdp answered with, or just its status when it isn't a success. */
  async #post(
    path: string,
    body: unknown,
    signal: AbortSignal,
  ): Promise<{ status: number; body?: Record<string, unknown> }> {
    const token = await this.#options.serviceToken?.();
    const res = await this.#fetch(`${this.#options.pdpUrl.replace(/\/$/, "")}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json", ...(token ? { authorization: `Bearer ${token}` } : {}) },
      body: JSON.stringify(body),
      signal,
    });
    if (!res.ok) return { status: res.status };
    const json: unknown = await res.json();
    return isRecord(json) ? { status: res.status, body: json } : { status: res.status };
  }

  async #check(req: CheckRequest, signal: AbortSignal): Promise<CheckResult> {
    let p: Principal;
    try {
      p = await this.#principal(req);
    } catch (e) {
      return { allowed: false, kind: "unauthenticated", reason: e instanceof Error ? e.message : String(e) };
    }
    const resource = { type: req.resource.type, id: req.resource.id };
    const key = JSON.stringify([p.principal, p.onBehalfOf ?? null, req.action, resource]);
    const body = {
      principal: p.principal,
      action: req.action,
      resource,
      context: req.context ?? {},
      entities: req.entities ?? [],
      ...(p.onBehalfOf ? { on_behalf_of: p.onBehalfOf } : {}),
      ...(p.humanPresent === undefined ? {} : { human_present: p.humanPresent }),
    };
    const note = (result: CheckResult, extra: Partial<DecisionNote> = {}) => {
      this.#options.onDecision?.({
        action: req.action,
        resource,
        principal: p.principal,
        ...(p.onBehalfOf ? { onBehalfOf: p.onBehalfOf } : {}),
        result: result.kind,
        ...extra,
      });
      return result;
    };

    let res: { status: number; body?: Record<string, unknown> };
    try {
      res = await this.#post("/v1/access/evaluation", body, signal);
    } catch (e) {
      const why = signal.aborted
        ? "awzy-pdp didn't answer within the budget"
        : `awzy-pdp can't be reached: ${String(e)}`;
      const fallback = this.#fallback(req, key, why);
      return note(fallback, fallback.allowed ? { fallback: fallback.fallback } : {});
    }
    if (res.status === 400 || res.status === 401 || res.status === 403) {
      return note({ allowed: false, kind: "unavailable", reason: `awzy-pdp refused the request (${res.status})` });
    }
    if (!res.body) {
      const fallback = this.#fallback(req, key, `awzy-pdp answered ${res.status}`);
      return note(fallback, fallback.allowed ? { fallback: fallback.fallback } : {});
    }

    const d = res.body;
    // A verdict it can't read is a deny, and a mode it can't read is enforce: both fail safe.
    const wireVerdict = text(d, "verdict");
    const verdict: Verdict = wireVerdict === "allow" || wireVerdict === "challenge" ? wireVerdict : "deny";
    const mode: Mode = text(d, "mode") === "monitor" ? "monitor" : "enforce";
    this.#lastMode = mode;
    const decisionQid = text(d, "qid") ?? "";
    const challengeQid = text(d, "challenge_qid");
    const extra = { decisionQid, mode, verdict };
    if (mode === "monitor" || verdict === "allow") {
      if (verdict === "allow") this.#allows.set(key, Date.now());
      return note({ allowed: true, kind: "allowed", decisionQid, mode, verdict }, extra);
    }
    if (verdict === "deny") {
      return note({ allowed: false, kind: "denied", decisionQid, policies: strings(d, "policies") }, extra);
    }
    if (!challengeQid) {
      return note(
        { allowed: false, kind: "unavailable", reason: "no challenge could be opened; try again shortly" },
        extra,
      );
    }
    const challenged = this.#challenged(p, decisionQid, challengeQid, kinds(d, "challenges"));
    // How long the person has, if awzy says so within what's left of the budget.
    try {
      const looked = await this.#lookup(p, challengeQid, 0, signal);
      if (looked.outcome === "pending" && looked.expiresAt) challenged.expiresAt = looked.expiresAt;
    } catch {
      // Without it, the challenge is still there to wait on.
    }
    return note(challenged, extra);
  }

  /** The gateway's outage rules (ADR 0030), with the app's fail-open actions (ADR 0112). */
  #fallback(req: CheckRequest, key: string, reason: string): CheckResult {
    if (this.#lastMode === "monitor") {
      return { allowed: true, kind: "allowed", mode: "monitor", verdict: "allow", fallback: "monitor" };
    }
    const tier = req.resource.tier;
    const allowedAt = this.#allows.get(key);
    const ttl = this.#options.cachedAllowTtlMs ?? 300_000;
    const open =
      this.#options.failOpen?.[req.action] === true &&
      tier !== undefined &&
      !ALWAYS_FAIL_CLOSED.includes(tier) &&
      (this.#options.failOpenTiers ?? []).includes(tier) &&
      allowedAt !== undefined &&
      Date.now() - allowedAt <= ttl;
    if (open) return { allowed: true, kind: "allowed", mode: "enforce", verdict: "allow", fallback: "cached_allow" };
    return { allowed: false, kind: "unavailable", reason };
  }

  #challenged(p: Principal, decisionQid: string, challengeQid: string, challenges: ChallengeKind[]): Challenged {
    return {
      allowed: false,
      kind: "challenged",
      decisionQid,
      challengeQid,
      challenges,
      wait: ({ timeoutMs, signal } = {}) => {
        const until = timeoutMs === undefined ? Infinity : Date.now() + timeoutMs;
        // One long poll after another, each at most 25 seconds, until it's answered or time's up.
        const poll = async (): Promise<ChallengeAnswer> => {
          const left = Math.max(0, Math.min(LONGEST_WAIT_SECS, Math.ceil((until - Date.now()) / 1000)));
          const signals = [AbortSignal.timeout((left + 10) * 1000), ...(signal ? [signal] : [])];
          const answer = await this.#lookup(p, challengeQid, left, AbortSignal.any(signals));
          return answer.outcome !== "pending" || Date.now() >= until ? answer : poll();
        };
        return poll();
      },
    };
  }

  async #lookup(p: Principal, challengeQid: string, waitSecs: number, signal: AbortSignal): Promise<ChallengeAnswer> {
    const res = await this.#post(
      "/v1/access/challenge",
      {
        challenge_qid: challengeQid,
        principal: p.principal,
        wait_secs: waitSecs,
        ...(p.onBehalfOf ? { on_behalf_of: p.onBehalfOf } : {}),
      },
      signal,
    );
    if (!res.body) throw new Error(`awzy-pdp answered ${res.status} about challenge ${challengeQid}`);
    const w = res.body;
    const expiresAt = text(w, "expires_at");
    const grantExpiresAt = text(w, "grant_expires_at");
    switch (text(w, "outcome")) {
      case "passed":
        return { outcome: "passed", ...(grantExpiresAt ? { grantExpiresAt } : {}) };
      case "rejected":
        return { outcome: "rejected", ...(expiresAt ? { expiresAt } : {}) };
      case "expired":
        return { outcome: "expired" };
      default:
        return { outcome: "pending", ...(expiresAt ? { expiresAt } : {}), pending: kinds(w, "pending") };
    }
  }
}
