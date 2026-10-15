// @vitest-environment jsdom
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockPayloadGenerator } from "relay-test-utils";
import { describe, expect, it, vi } from "vitest";

import { Configure } from "./ConnectorsPage";
import { renderWithRelay } from "./testing";

vi.mock("@tanstack/react-router", () => ({
  getRouteApi: () => ({ useLoaderData: () => null }),
  useRouter: () => ({ invalidate: () => Promise.resolve() }),
}));

const JIRA = { name: "JIRA" as const, key: "jira", title: "Jira", credential: "API token" };

function renderConfigure(credentialsSupported: boolean) {
  const onClose = vi.fn();
  const environment = renderWithRelay(
    <Configure connector={JIRA} current={undefined} credentialsSupported={credentialsSupported} onClose={onClose} />,
  );
  return { environment, onClose };
}

describe("Configure", () => {
  // It types about 70 characters, which took over Vitest's 5 s default on a busy CI runner: no
  // pause between keys, and more time.
  it("saves Jira's settings as the API takes them, and closes", { timeout: 20_000 }, async () => {
    const user = userEvent.setup({ delay: null });
    const { environment, onClose } = renderConfigure(true);

    await user.type(screen.getByRole("textbox", { name: /Site/ }), "https://example.atlassian.net");
    await user.type(screen.getByRole("textbox", { name: /Account/ }), "sync@example.com");
    await user.type(screen.getByRole("textbox", { name: /Projects/ }), "ABC, DEF,");
    await user.type(screen.getByLabelText(/API token/), "token-1");
    await user.click(screen.getByRole("button", { name: "Save" }));

    const operation = environment.mock.getMostRecentOperation();
    expect(operation.request.variables).toEqual({
      input: {
        name: "JIRA",
        enabled: true,
        settings: {
          url: "https://example.atlassian.net",
          user: "sync@example.com",
          projects: ["ABC", "DEF"],
          watchers: false,
          interval_secs: 300,
        },
        credential: "token-1",
      },
    });
    environment.mock.resolve(
      operation,
      MockPayloadGenerator.generate(operation, {
        ConfigureConnectorPayload: () => ({ challengeQid: null, connector: { name: "JIRA" } }),
      }),
    );
    await vi.waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("says when the install can't keep credentials", () => {
    renderConfigure(false);
    expect(screen.getByText("Credentials can't be set here")).toBeTruthy();
    expect(screen.queryByLabelText(/API token/)).toBeNull();
  });
});
