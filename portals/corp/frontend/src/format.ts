/** Shared display helpers. */

import type { Tone } from "@awzy/ui";

import type { SupportSessionStatus } from "./__generated__/SupportSection_install.graphql";

// An install that has never reported is a warning, not a neutral state: it was registered and
// something is keeping its heartbeat from arriving.
export const HEALTH: Record<string, { label: string; tone: Tone }> = {
  OK: { label: "Healthy", tone: "success" },
  DEGRADED: { label: "Degraded", tone: "warning" },
  NOT_REPORTING: { label: "Not reporting", tone: "danger" },
  NEVER_REPORTED: { label: "Never reported", tone: "warning" },
};

export function healthOf(h: string): { label: string; tone: Tone } {
  return HEALTH[h] ?? { label: h.toLowerCase(), tone: "neutral" };
}

/** A UtcTimestamp in the viewer's locale and time zone. */
export function formatTime(utc: string): string {
  return new Date(utc).toLocaleString();
}

/** "12s ago", "4m ago", "3h ago", or the date for anything older than a day. */
export function ago(utc: string | null | undefined, now = Date.now()): string {
  if (!utc) return "never";
  const s = Math.max(0, Math.round((now - new Date(utc).getTime()) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return new Date(utc).toLocaleDateString();
}

/** The release channels staff choose from, for a Select. */
/** The release channels' names, for narrowing a Select's value without a cast. */
export const CHANNEL_NAMES = ["FAST", "STABLE", "PINNED"] as const;

export const CHANNELS = [
  { value: "FAST", label: "fast" },
  { value: "STABLE", label: "stable" },
  { value: "PINNED", label: "pinned" },
];

export function channelLabel(channel: string, pinned: string | null | undefined): string {
  return channel === "PINNED" ? `pinned ${pinned ?? ""}`.trim() : channel.toLowerCase();
}

/** `sha256:3f1a…9c0b`: enough to tell digests apart. */
export function shortDigest(d: string | null | undefined): string {
  if (!d) return "";
  const hex = d.replace(/^sha256:/, "");
  return `sha256:${hex.slice(0, 4)}…${hex.slice(-4)}`;
}

/** A support session's status as staff read it (ADR 0095), one entry per status in the schema. */
const SUPPORT_STATUS: Record<Exclude<SupportSessionStatus, "%future added value">, { label: string; tone: Tone }> = {
  REQUESTED: { label: "Waiting for the client", tone: "warning" },
  APPROVED: { label: "Approved", tone: "success" },
  REJECTED: { label: "Rejected", tone: "neutral" },
  ENDED: { label: "Ended by the client", tone: "neutral" },
  EXPIRED: { label: "Expired", tone: "neutral" },
};

export function supportStatus(status: SupportSessionStatus): { label: string; tone: Tone } {
  return status === "%future added value" ? { label: "Unknown", tone: "neutral" } : SUPPORT_STATUS[status];
}
