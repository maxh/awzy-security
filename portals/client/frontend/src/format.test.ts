import { describe, expect, it } from "vitest";

import { ago, entityLabel, prettyJson, verdictLabel } from "./format";

describe("verdictLabel", () => {
  it("says what a monitored decision would have done", () => {
    expect(verdictLabel("DENY", "MONITOR")).toBe("would deny");
    expect(verdictLabel("ALLOW", "MONITOR")).toBe("allow");
    expect(verdictLabel("DENY", "ENFORCE")).toBe("deny");
  });

  it("names every challenge a decision asked for", () => {
    expect(verdictLabel("CHALLENGE", "ENFORCE", ["SECURITY_KEY", "APPROVAL"])).toBe(
      "challenge: SECURITY_KEY + APPROVAL",
    );
  });
});

describe("ago", () => {
  const now = Date.parse("2026-10-11T12:00:00Z");

  it("rounds to the largest whole unit under a day", () => {
    expect(ago("2026-10-11T11:59:48Z", now)).toBe("12s ago");
    expect(ago("2026-10-11T11:56:00Z", now)).toBe("4m ago");
    expect(ago("2026-10-11T09:00:00Z", now)).toBe("3h ago");
  });

  it("never says a time in the future is negative", () => {
    expect(ago("2026-10-11T12:00:30Z", now)).toBe("0s ago");
  });
});

describe("prettyJson", () => {
  it("indents JSON and leaves anything else as it was", () => {
    expect(prettyJson('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(prettyJson("not json")).toBe("not json");
  });
});

it("labels an entity as Cedar writes it", () => {
  expect(entityLabel({ type: "Human", id: "alice" })).toBe("Human::alice");
});
