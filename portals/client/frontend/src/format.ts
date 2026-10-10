/** Shared display helpers. */

export const VERDICT_COLOR = { ALLOW: "teal", DENY: "red", CHALLENGE: "yellow" } as const;

/** "deny", or "would deny" when the decision was only monitored; plus every challenge required. */
export function verdictLabel(verdict: string, mode: string, challenges: ReadonlyArray<string> = []): string {
  const v = verdict.toLowerCase();
  const label = mode === "MONITOR" && v !== "allow" ? `would ${v}` : v;
  return challenges.length > 0 ? `${label}: ${challenges.join(" + ")}` : label;
}

/** A UtcTimestamp in the viewer's locale and time zone. */
export function formatTime(utc: string): string {
  return new Date(utc).toLocaleString();
}

export function entityLabel(e: { readonly type: string; readonly id: string }): string {
  return `${e.type}::${e.id}`;
}

/** Pretty-prints JSON text; returns it unchanged if it doesn't parse. */
export function prettyJson(text: string): string {
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
}
