import type { EntityRef } from "./types.ts";

/** A literal a filter compares with: a boolean, a whole number, a string, or an entity. */
export type Literal = boolean | number | string | EntityRef;

export type Cmp = "eq" | "ne" | "lt" | "le" | "gt" | "ge";

/**
 * Which rows of a type a principal may act on, from a query plan. `attr` is a path into the
 * row's attributes. A row with the attribute missing never matches, as SQL's `NULL` doesn't.
 */
export type Filter =
  | { op: "true" }
  | { op: "false" }
  | { op: "and"; args: Filter[] }
  | { op: "or"; args: Filter[] }
  | { op: "not"; arg: Filter }
  | { op: "compare"; attr: string[]; cmp: Cmp; value: Literal }
  | { op: "has"; attr: string[] }
  | { op: "contains"; attr: string[]; value: Literal }
  | { op: "contains_any"; attr: string[]; values: Literal[] }
  | { op: "contains_all"; attr: string[]; values: Literal[] }
  | { op: "like"; attr: string[]; pattern: string }
  /** The row is `ancestor` or under it in the app's hierarchy. */
  | { op: "in"; ancestor: EntityRef }
  /** The row is the entity `id`. */
  | { op: "is"; id: EntityRef }
  /** Depends on something awzy works out per row: check these rows one by one. */
  | { op: "unknown"; name: string }
  /** A condition the tree can't express: check these rows one by one. */
  | { op: "unmappable"; cedar: string };

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

const isEntity = (v: unknown): v is EntityRef =>
  isRecord(v) && typeof v["type"] === "string" && typeof v["id"] === "string";

const isLiteral = (v: unknown): v is Literal =>
  typeof v === "boolean" || typeof v === "number" || typeof v === "string" || isEntity(v);

const isPath = (v: unknown): v is string[] => Array.isArray(v) && v.length > 0 && v.every((p) => typeof p === "string");

const CMPS: ReadonlySet<string> = new Set(["eq", "ne", "lt", "le", "gt", "ge"]);
const isCmp = (v: unknown): v is Cmp => typeof v === "string" && CMPS.has(v);

/**
 * Reads a filter from awzy's JSON. Anything it doesn't recognize, as from a newer awzy, becomes
 * `unmappable`, so the rows are checked one by one rather than shown.
 */
export function parseFilter(v: unknown): Filter {
  const unmappable = (): Filter => ({ op: "unmappable", cedar: JSON.stringify(v) });
  if (!isRecord(v)) return unmappable();
  const op = v["op"];
  switch (op) {
    case "true":
    case "false":
      return { op };
    case "and":
    case "or": {
      const args = v["args"];
      return Array.isArray(args) ? { op, args: args.map(parseFilter) } : unmappable();
    }
    case "not":
      return { op, arg: parseFilter(v["arg"]) };
    case "compare": {
      const { attr, cmp, value } = v;
      return isPath(attr) && isCmp(cmp) && isLiteral(value) ? { op, attr, cmp, value } : unmappable();
    }
    case "has":
      return isPath(v["attr"]) ? { op, attr: v["attr"] } : unmappable();
    case "contains": {
      const { attr, value } = v;
      return isPath(attr) && isLiteral(value) ? { op, attr, value } : unmappable();
    }
    case "contains_any":
    case "contains_all": {
      const { attr, values } = v;
      return isPath(attr) && Array.isArray(values) && values.every(isLiteral) ? { op, attr, values } : unmappable();
    }
    case "like": {
      const { attr, pattern } = v;
      return isPath(attr) && typeof pattern === "string" ? { op, attr, pattern } : unmappable();
    }
    case "in":
      return isEntity(v["ancestor"]) ? { op, ancestor: v["ancestor"] } : unmappable();
    case "is":
      return isEntity(v["id"]) ? { op, id: v["id"] } : unmappable();
    case "unknown":
      return typeof v["name"] === "string" ? { op, name: v["name"] } : unmappable();
    default:
      return unmappable();
  }
}

/** Whether some row has to be checked one by one. */
export function needsRowChecks(f: Filter): boolean {
  switch (f.op) {
    case "unknown":
    case "unmappable":
      return true;
    case "and":
    case "or":
      return f.args.some(needsRowChecks);
    case "not":
      return needsRowChecks(f.arg);
    default:
      return false;
  }
}
