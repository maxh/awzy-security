import { describe, expect, it } from "vitest";

import { type Filter, parseFilter } from "./filter.ts";
import { likePattern, type SqlMapping, toPostgres } from "./sql.ts";

const customers: SqlMapping = {
  id: "customers.id",
  columns: { tenant: "tenant", tier: "tier", labels: "labels", owner: "owner_id" },
  ancestors: { table: "customer_regions", row: "customer_id", ancestor: "region_id" },
};

describe("toPostgres", () => {
  it("maps comparisons, sets, and hierarchy membership with parameters", () => {
    const filter: Filter = {
      op: "and",
      args: [
        { op: "compare", attr: ["tenant"], cmp: "eq", value: "acme" },
        { op: "not", arg: { op: "contains", attr: ["labels"], value: "legal-hold" } },
        { op: "in", ancestor: { type: "Region", id: "emea" } },
        { op: "compare", attr: ["owner"], cmp: "eq", value: { type: "Human", id: "dana" } },
      ],
    };
    expect(toPostgres(filter, customers)).toEqual({
      where:
        "(tenant = $1 AND NOT $2 = ANY(labels) AND (customers.id = $3 OR EXISTS (SELECT 1 FROM customer_regions " +
        "WHERE customer_regions.customer_id = customers.id AND customer_regions.region_id = $3)) AND owner_id = $4)",
      params: ["acme", "legal-hold", "emea", "dana"],
      needsRowChecks: false,
      unmapped: [],
    });
  });

  it("keeps the clause a superset where it can't map, and says so", () => {
    const unknown: Filter = { op: "unknown", name: "context.assigned" };
    const allowed = toPostgres({ op: "or", args: [unknown, { op: "has", attr: ["nickname"] }] }, customers);
    expect(allowed.where).toBe("(TRUE OR TRUE)");
    expect(allowed).toMatchObject({ needsRowChecks: true });
    expect(allowed.unmapped).toEqual(["context.assigned is worked out by awzy for each row", "no column for nickname"]);
    // Under a not, an unmapped part must not hide rows either: it becomes FALSE.
    expect(toPostgres({ op: "not", arg: unknown }, customers).where).toBe("NOT FALSE");
  });

  it("numbers parameters from where the app's own end", () => {
    const sql = toPostgres({ op: "compare", attr: ["tier"], cmp: "lt", value: 3 }, customers, 4);
    expect(sql).toMatchObject({ where: "tier < $4", params: [3] });
  });

  it("turns Cedar's like into SQL's, escaping SQL's own wildcards", () => {
    expect(likePattern("*.pdf")).toBe("%.pdf");
    expect(likePattern("50%_off\\*")).toBe("50\\%\\_off*");
  });
});

describe("parseFilter", () => {
  it("reads what it knows and marks the rest unmappable", () => {
    expect(parseFilter({ op: "compare", attr: ["tier"], cmp: "ge", value: 2 })).toEqual({
      op: "compare",
      attr: ["tier"],
      cmp: "ge",
      value: 2,
    });
    expect(parseFilter({ op: "regex", attr: ["x"] })).toMatchObject({ op: "unmappable" });
    expect(parseFilter("nonsense")).toMatchObject({ op: "unmappable" });
  });
});
