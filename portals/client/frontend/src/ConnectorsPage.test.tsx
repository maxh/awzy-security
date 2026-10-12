// @vitest-environment jsdom
import { MantineProvider } from "@mantine/core";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RelayEnvironmentProvider } from "react-relay";
import { createMockEnvironment, MockPayloadGenerator } from "relay-test-utils";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { Configure } from "./ConnectorsPage";

// relay-test-utils' mock environment calls jest.fn; Vitest's vi is the same API.
vi.hoisted(() => Object.assign(globalThis, { jest: vi }));

vi.mock("@tanstack/react-router", () => ({
  getRouteApi: () => ({ useLoaderData: () => null }),
  useRouter: () => ({ invalidate: () => Promise.resolve() }),
}));

beforeAll(() => {
  // Mantine reads these, and jsdom has neither.
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    // MediaQueryList still declares the old listener methods, so a stand-in has them too.
    // oxlint-disable-next-line typescript/no-deprecated
    addListener: () => {},
    // oxlint-disable-next-line typescript/no-deprecated
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  });
  globalThis.ResizeObserver = class {
    observe() {}
    unobserve() {}
    disconnect() {}
  };
});

// Without Vitest globals, Testing Library doesn't unmount between tests by itself.
afterEach(cleanup);

const JIRA = { name: "JIRA" as const, key: "jira", title: "Jira", credential: "API token" };

function renderConfigure(credentialsSupported: boolean) {
  const environment = createMockEnvironment();
  const onClose = vi.fn();
  render(
    <MantineProvider>
      <RelayEnvironmentProvider environment={environment}>
        <Configure connector={JIRA} current={undefined} credentialsSupported={credentialsSupported} onClose={onClose} />
      </RelayEnvironmentProvider>
    </MantineProvider>,
  );
  return { environment, onClose };
}

describe("Configure", () => {
  it("saves Jira's settings as the API takes them, and closes", async () => {
    const user = userEvent.setup();
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
