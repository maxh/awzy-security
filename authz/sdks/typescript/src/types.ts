import type { Filter } from "./filter.ts";

/** A typed reference to an entity: `{ type: "Human", id: "alice" }`. */
export interface EntityRef {
  type: string;
  id: string;
}

/** Who a check is about, from a validated token: the principal, and the person it acts for. */
export interface Principal {
  principal: EntityRef;
  onBehalfOf?: EntityRef;
  /** Whether a person is there to answer a challenge; unset leaves it to the PDP. */
  humanPresent?: boolean;
  /**
   * The work item (a ticket or task, in your own systems' IDs) the principal says it's doing,
   * from the token's `awzy_work_item_id`. The PDP then decides assignment by that item alone.
   */
  workItemId?: string;
}

/** A resource, with its tier when the app knows it: only a known tier can fail open. */
export interface Resource extends EntityRef {
  tier?: string;
}

export type ChallengeKind = "justification" | "verification" | "approval" | "biometric";
export type Verdict = "allow" | "deny" | "challenge";
export type Mode = "monitor" | "enforce";

/** One call to check: the caller's token or an already validated principal, and what it does. */
export interface CheckRequest {
  /** The caller's `Authorization` header (`Bearer …`). Validated before anything is asked. */
  authorization?: string;
  /** Instead of `authorization`: a principal the app validated with this SDK's verifier. */
  principal?: Principal;
  /** The tool function or operation, e.g. `update_invoice`. */
  action: string;
  resource: Resource;
  context?: Record<string, unknown>;
  /** Entities of types the app owns, in Cedar's entity shape. awzy's own come from awzy. */
  entities?: unknown[];
}

/** The call may go ahead. In monitor mode, `verdict` is what enforcing would have done. */
export interface Allowed {
  allowed: true;
  kind: "allowed";
  decisionQid?: string;
  mode: Mode;
  verdict: Verdict;
  /** Set when the PDP couldn't be asked: why the call is allowed anyway. */
  fallback?: "monitor" | "cached_allow";
}

export interface Denied {
  allowed: false;
  kind: "denied";
  decisionQid: string;
  policies: string[];
}

/** The person must answer first. Wait for their answer, then retry the same call. */
export interface Challenged {
  allowed: false;
  kind: "challenged";
  decisionQid: string;
  challengeQid: string;
  challenges: ChallengeKind[];
  /** When it expires unanswered, when awzy said in time. */
  expiresAt?: string;
  /** Resolves when the person answers, the challenge expires, or `timeoutMs` passes (pending). */
  wait(options?: { timeoutMs?: number; signal?: AbortSignal }): Promise<ChallengeAnswer>;
}

/** awzy couldn't decide, and nothing allows the call without it. Retry shortly. */
export interface Unavailable {
  allowed: false;
  kind: "unavailable";
  reason: string;
}

/** The caller's token isn't valid: nothing was asked. */
export interface Unauthenticated {
  allowed: false;
  kind: "unauthenticated";
  reason: string;
}

export type CheckResult = Allowed | Denied | Challenged | Unavailable | Unauthenticated;

export type ChallengeAnswer =
  | { outcome: "passed"; grantExpiresAt?: string }
  | { outcome: "rejected"; expiresAt?: string }
  | { outcome: "expired" }
  | { outcome: "pending"; expiresAt?: string; pending: ChallengeKind[] };

/** What the app's logs and traces get for each decision (ADR 0045, rule 6). */
export interface DecisionNote {
  decisionQid?: string;
  action: string;
  resource: EntityRef;
  principal: EntityRef;
  onBehalfOf?: EntityRef;
  result: CheckResult["kind"];
  mode?: Mode;
  verdict?: Verdict;
  fallback?: Allowed["fallback"];
}

/** What a list or search asks: which rows of `resourceType` may this principal act on? */
export interface PlanRequest {
  authorization?: string;
  principal?: Principal;
  action: string;
  /** The entity type the app lists, e.g. `Customer`. */
  resourceType: string;
  context?: Record<string, unknown>;
  entities?: unknown[];
  /** The first page's plan, to page through with the same filter. */
  planQid?: string;
}

export type PlanOutcome = "always_allowed" | "always_denied" | "challenge" | "conditional";

/** awzy's answer: what to show, as a filter, and what it rests on. */
export interface Plan {
  ok: true;
  qid: string;
  outcome: PlanOutcome;
  filter: Filter;
  /**
   * The filter to apply now, with the mode applied: in monitor every row (`true`), else the
   * plan's filter, or no row while a challenge is pending.
   */
  apply: Filter;
  challenges: ChallengeKind[];
  challengeCouldWiden: boolean;
  /** Some rows must be checked one by one with {@link CheckResult}s. */
  needsRowChecks: boolean;
  attributes: string[];
  mode: Mode;
  maxAgeSecs: number;
  pinned: boolean;
}

export type PlanResult = Plan | Unavailable | Unauthenticated;

/** Tiers that never fail open, whatever the settings say. */
export const ALWAYS_FAIL_CLOSED: readonly string[] = ["confidential", "crown_jewels"];
