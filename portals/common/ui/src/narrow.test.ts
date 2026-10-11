import { describe, expect, it } from "vitest";

import { isOneOf, isRecord, messageOf, oneOf } from "./narrow";

describe("oneOf", () => {
  const MODES = ["ENFORCE", "MONITOR"] as const;

  it("narrows only the listed strings", () => {
    expect(oneOf(MODES, "MONITOR")).toBe("MONITOR");
    expect(oneOf(MODES, "%future added value")).toBeNull();
    expect(oneOf(MODES, 1)).toBeNull();
    expect(isOneOf(MODES, "ENFORCE")).toBe(true);
  });
});

describe("isRecord", () => {
  it("is true for plain objects only", () => {
    expect(isRecord({ a: 1 })).toBe(true);
    expect(isRecord([])).toBe(false);
    expect(isRecord(null)).toBe(false);
    expect(isRecord("x")).toBe(false);
  });
});

it("reads a message from whatever was thrown", () => {
  expect(messageOf(new Error("boom"))).toBe("boom");
  expect(messageOf("plain")).toBe("plain");
});
