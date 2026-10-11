/**
 * Whether `value` is one of `values`: for narrowing a string from a Select, a URL, or a Relay
 * enum (which also allows "%future added value") without an `as`, which the lint forbids.
 */
export function isOneOf<const T extends string>(values: readonly T[], value: unknown): value is T {
  return typeof value === "string" && (values as readonly string[]).includes(value);
}

/** `value` if it's one of `values`, else null. */
export function oneOf<const T extends string>(values: readonly T[], value: unknown): T | null {
  return isOneOf(values, value) ? value : null;
}

/** Whether `value` is a plain JSON object, so its fields can be checked one by one. */
export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** An error's message, whatever was thrown. */
export function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
