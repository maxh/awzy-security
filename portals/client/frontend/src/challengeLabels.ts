/** How the Challenges page names challenges and requests, shared by its sections. */

import { entityLabel } from "./format";

/** The challenge a notification linked to (`/challenges?challenge=<qid>`), highlighted. Read as
 * the page renders, since signing in restores the link only after the app loads. */
export function linkedChallenge(): string | null {
  return new URLSearchParams(window.location.search).get("challenge");
}
export const LINKED = { outline: "2px solid var(--mantine-color-yellow-5)", borderRadius: 4 };

export type Ref = { readonly type: string; readonly id: string } | null | undefined;

/** "alice read Resource::board-deck", or "sales-genie for bob read …". */
export function requestLabel(c: { principal: Ref; onBehalfOf: Ref; action: string; resource: Ref }): string {
  const who = c.principal ? c.principal.id : "someone";
  const forWhom = c.onBehalfOf ? ` for ${c.onBehalfOf.id}` : "";
  return `${who}${forWhom} ${c.action} ${c.resource ? entityLabel(c.resource) : ""}`;
}

export function kindLabel(kind: string): string {
  return kind.toLowerCase();
}
