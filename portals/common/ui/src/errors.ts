import { oneOf } from "./narrow";

/** The kinds of failure the portals' GraphQL errors carry as a `code` extension (#224). */
export const ERROR_CODES = [
  "UNAUTHENTICATED",
  "FORBIDDEN",
  "CHALLENGE_REQUIRED",
  "INVALID_INPUT",
  "NOT_FOUND",
  "UNAVAILABLE",
] as const;

export type ErrorCode = (typeof ERROR_CODES)[number];

/** What kind of failure `e` is, when it came from a portal backend that said. */
export function errorCode(e: unknown): ErrorCode | null {
  return oneOf(ERROR_CODES, typeof e === "object" && e !== null && "code" in e ? e.code : null);
}
