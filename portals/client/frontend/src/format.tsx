/** Shared display helpers. */

import { StatusBadge, type Tone } from "@awzy/ui";
import { Text, Tooltip } from "@mantine/core";

const VERDICT_TONE: Record<string, Tone> = { ALLOW: "success", DENY: "danger", CHALLENGE: "warning" };

/** "deny", or "would deny" when the decision was only monitored; plus every challenge required. */
export function verdictLabel(verdict: string, mode: string, challenges: ReadonlyArray<string> = []): string {
  const v = verdict.toLowerCase();
  const label = mode === "MONITOR" && v !== "allow" ? `would ${v}` : v;
  return challenges.length > 0 ? `${label}: ${challenges.join(" + ")}` : label;
}

/** In place of a field a policy hides from the viewer (ADR 0103). */
export function Hidden() {
  return (
    <Text size="sm" c="dimmed" fs="italic">
      Hidden by a policy
    </Text>
  );
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

/** "12s ago", "4m ago", "3h ago", or the date for anything older than a day. */
export function ago(utc: string, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - new Date(utc).getTime()) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(utc).toLocaleDateString();
}

/** A time as "4m ago", with the full time on hover. */
export function Time({ at }: { at: string }) {
  return (
    <Tooltip label={formatTime(at)}>
      <Text span size="sm">
        {ago(at)}
      </Text>
    </Tooltip>
  );
}

/** An entity as its ID, with its type dimmed after it: `alice  Human`. */
export function Entity({ e }: { e: { readonly type: string; readonly id: string } }) {
  return (
    <Text span size="sm">
      <Text span inherit fw={500}>
        {e.id}
      </Text>{" "}
      <Text span size="xs" c="dimmed">
        {e.type}
      </Text>
    </Text>
  );
}

/** The verdict as a colored badge; monitored ones ("would deny") are outlined. */
export function VerdictBadge({
  verdict,
  mode,
  challenges = [],
}: {
  verdict: string;
  mode: string;
  challenges?: ReadonlyArray<string>;
}) {
  const label = verdictLabel(verdict, mode, challenges);
  return (
    <StatusBadge tone={VERDICT_TONE[verdict] ?? "neutral"}>
      {label.charAt(0).toUpperCase() + label.slice(1)}
    </StatusBadge>
  );
}
