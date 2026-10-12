import type { Filter, Literal } from "./filter.ts";
import type { EntityRef } from "./types.ts";

/** How a type's attributes sit in a Postgres table. */
export interface SqlMapping {
  /**
   * The row's ID column, for `is` and `in`, qualified with its table (`customers.id`) so the
   * ancestor join's subquery can't mistake it for a column of its own.
   */
  id: string;
  /** Each attribute path, joined with dots, to its column: `{ "tenant": "tenant", "labels": "labels" }`. */
  columns: Record<string, string>;
  /**
   * For `in`: a table holding each row's ancestors (its ancestor closure), one per row, so
   * `resource in Region::"emea"` is a join on it.
   */
  ancestors?: { table: string; row: string; ancestor: string };
  /** An entity literal as the column holds it. Default: its `id`. */
  entity?: (e: EntityRef) => string | number;
}

export interface Sql {
  /** A boolean expression for a `WHERE` clause, with `$1`, `$2`, … for `params`. */
  where: string;
  params: unknown[];
  /**
   * Some of the filter couldn't be mapped, or depends on what awzy works out per row. `where`
   * is then a superset: check each row it returns one by one before showing it.
   */
  needsRowChecks: boolean;
  /** What couldn't be mapped, and why. */
  unmapped: string[];
}

/** A column or table name, quoted unless it's a plain lowercase name (or `table.column`). */
function quote(name: string): string {
  return /^[a-z_][a-z0-9_]*(\.[a-z_][a-z0-9_]*)?$/.test(name) ? name : `"${name.replaceAll('"', '""')}"`;
}

/**
 * Cedar's `like` pattern as SQL's: `*` is `%`, `\*` a literal star, and SQL's own `%`, `_`, and
 * `\` escaped.
 */
export function likePattern(cedar: string): string {
  let out = "";
  for (let i = 0; i < cedar.length; i += 1) {
    const c = cedar[i];
    if (c === "\\" && cedar[i + 1] === "*") {
      out += "*";
      i += 1;
    } else if (c === "*") {
      out += "%";
    } else if (c === "%" || c === "_" || c === "\\") {
      out += `\\${c}`;
    } else {
      out += c;
    }
  }
  return out;
}

/**
 * A plan's filter as a Postgres `WHERE` clause: the reference mapper (ADR 0113). What it can't
 * map is replaced by whatever keeps the clause a superset of the rows the filter matches (`TRUE`
 * where it allows rows, `FALSE` under a `not`), and reported, so the app checks just those rows.
 */
export function toPostgres(filter: Filter, mapping: SqlMapping, firstParam = 1): Sql {
  const params: unknown[] = [];
  const unmapped: string[] = [];
  const param = (v: unknown) => {
    params.push(v);
    return `$${firstParam + params.length - 1}`;
  };
  const value = (l: Literal): unknown => (typeof l === "object" ? (mapping.entity ?? ((e) => e.id))(l) : l);
  const column = (attr: string[]) => {
    const c = mapping.columns[attr.join(".")];
    return c === undefined ? undefined : quote(c);
  };
  // `positive`: under an even number of `not`s, where an unmapped part must allow rows.
  const map = (f: Filter, positive: boolean): string => {
    const cant = (why: string) => {
      unmapped.push(why);
      return positive ? "TRUE" : "FALSE";
    };
    const col = (attr: string[], build: (c: string) => string) => {
      const c = column(attr);
      return c === undefined ? cant(`no column for ${attr.join(".")}`) : build(c);
    };
    switch (f.op) {
      case "true":
        return "TRUE";
      case "false":
        return "FALSE";
      case "and":
        return f.args.length === 0 ? "TRUE" : `(${f.args.map((a) => map(a, positive)).join(" AND ")})`;
      case "or":
        return f.args.length === 0 ? "FALSE" : `(${f.args.map((a) => map(a, positive)).join(" OR ")})`;
      case "not":
        return `NOT ${map(f.arg, !positive)}`;
      case "compare": {
        const op = { eq: "=", ne: "<>", lt: "<", le: "<=", gt: ">", ge: ">=" }[f.cmp];
        return col(f.attr, (c) => `${c} ${op} ${param(value(f.value))}`);
      }
      case "has":
        return col(f.attr, (c) => `${c} IS NOT NULL`);
      case "contains":
        return col(f.attr, (c) => `${param(value(f.value))} = ANY(${c})`);
      case "contains_any":
        return col(f.attr, (c) => `${c} && ${param(f.values.map(value))}`);
      case "contains_all":
        return col(f.attr, (c) => `${c} @> ${param(f.values.map(value))}`);
      case "like":
        return col(f.attr, (c) => `${c} LIKE ${param(likePattern(f.pattern))}`);
      case "is":
        return `${quote(mapping.id)} = ${param(value(f.id))}`;
      case "in": {
        const a = mapping.ancestors;
        if (!a) return cant(`no ancestors table for in ${f.ancestor.type}::${f.ancestor.id}`);
        const p = param(value(f.ancestor));
        const id = quote(mapping.id);
        return `(${id} = ${p} OR EXISTS (SELECT 1 FROM ${quote(a.table)} WHERE ${quote(a.table)}.${quote(a.row)} = ${id} AND ${quote(a.table)}.${quote(a.ancestor)} = ${p}))`;
      }
      case "unknown":
        return cant(`${f.name} is worked out by awzy for each row`);
      case "unmappable":
        return cant(`Cedar the filter can't express: ${f.cedar}`);
      default:
        return cant(`a filter node it doesn't know: ${JSON.stringify(f satisfies never)}`);
    }
  };
  const where = map(filter, true);
  return { where, params, needsRowChecks: unmapped.length > 0, unmapped };
}
