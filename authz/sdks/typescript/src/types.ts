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

/** Tiers that never fail open, whatever the settings say. */
export const ALWAYS_FAIL_CLOSED: readonly string[] = ["confidential", "crown_jewels"];
