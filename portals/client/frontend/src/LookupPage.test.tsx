// @vitest-environment jsdom
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockPayloadGenerator } from "relay-test-utils";
import { describe, expect, it } from "vitest";

import { LookupPage } from "./LookupPage";
import { renderWithRelay } from "./testing";

describe("LookupPage", () => {
  it("shows a principal as the PDPs see it: groups, risk, and attributes", async () => {
    const user = userEvent.setup();
    const environment = renderWithRelay(<LookupPage />);
    await user.type(screen.getByRole("textbox", { name: "ID" }), "nora");
    await user.click(screen.getByRole("button", { name: "Look up" }));

    const operation = environment.mock.getMostRecentOperation();
    expect(operation.request.variables).toEqual({ type: "HUMAN", id: "nora" });
    environment.mock.resolve(
      operation,
      MockPayloadGenerator.generate(operation, {
        PrincipalDetail: () => ({
          known: true,
          attributes: { department: "finance" },
          groups: ["analysts", "everyone"],
          riskScore: 72,
        }),
      }),
    );
    expect(await screen.findByText("analysts, everyone")).toBeTruthy();
    expect(screen.getByText("72")).toBeTruthy();
    expect(screen.getByText(/"department": "finance"/)).toBeTruthy();
  });

  it("says when awzy doesn't know the principal", async () => {
    const user = userEvent.setup();
    const environment = renderWithRelay(<LookupPage />);
    await user.type(screen.getByRole("textbox", { name: "ID" }), "ghost{Enter}");
    environment.mock.resolveMostRecentOperation((op) =>
      MockPayloadGenerator.generate(op, {
        PrincipalDetail: () => ({ known: false, attributes: null, groups: [], riskScore: null }),
      }),
    );
    expect(await screen.findByText("awzy doesn't know ghost")).toBeTruthy();
  });
});
