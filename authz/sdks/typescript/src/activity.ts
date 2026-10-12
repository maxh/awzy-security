import type { EntityRef } from "./types.ts";

/** One thing a principal did, in awzy-intake's app event schema. No argument values. */
export interface Activity {
  at?: Date;
  principal: EntityRef;
  onBehalfOf?: EntityRef;
  action: string;
  resource?: EntityRef;
  outcome?: "success" | "failure";
  attributes?: Record<string, unknown>;
}

export interface ActivityOptions {
  /** awzy-intake's base URL. */
  url: string;
  /** Its bearer token for this app. */
  token: string;
  /** Names the app in each event, e.g. `acme-crm`. */
  system: string;
  fetch?: typeof fetch;
}

/** Events queued before new ones are dropped. */
const QUEUE = 10_000;
/** Most events in one request; awzy-intake takes up to 1,000. */
const BATCH = 500;
const EVERY_MS = 1_000;

/**
 * Records what principals do through the app in awzy-intake (ADR 0084), so awzy-reasoner scores
 * an agent's tool calls like any other activity. Best effort: queued, sent a batch a second, and
 * dropped when the queue is full or awzy-intake can't be reached. Recording never slows a call.
 */
export class ActivityLog {
  readonly #options: ActivityOptions;
  readonly #queue: Activity[] = [];
  #timer: ReturnType<typeof setInterval> | undefined;

  constructor(options: ActivityOptions) {
    this.#options = options;
  }

  /** Queues `activity` to be sent. Never waits. */
  record(activity: Activity): void {
    if (this.#queue.length >= QUEUE) return;
    this.#queue.push({ ...activity, at: activity.at ?? new Date() });
    if (!this.#timer) {
      this.#timer = setInterval(() => void this.flush(), EVERY_MS);
      // Never keeps a script running just to send activity.
      this.#timer.unref?.();
    }
  }

  /** Sends what's queued now. Call before a script exits. */
  async flush(): Promise<void> {
    const batch = this.#queue.splice(0, BATCH);
    if (batch.length === 0) return;
    const events = batch.map((a) => {
      const e: Record<string, unknown> = {
        at: (a.at ?? new Date()).toISOString(),
        system: this.#options.system,
        principal: a.principal,
        action: a.action,
        attributes: a.attributes ?? {},
      };
      if (a.onBehalfOf) e["on_behalf_of"] = a.onBehalfOf;
      if (a.resource) e["resource"] = a.resource;
      if (a.outcome) e["outcome"] = a.outcome;
      return e;
    });
    try {
      await (this.#options.fetch ?? fetch)(`${this.#options.url.replace(/\/$/, "")}/v1/events`, {
        method: "POST",
        headers: { "content-type": "application/json", authorization: `Bearer ${this.#options.token}` },
        body: JSON.stringify({ events }),
        signal: AbortSignal.timeout(10_000),
      });
    } catch {
      // Best effort: dropped.
    }
    // A batch at a time, until the queue is empty.
    return this.flush();
  }
}
