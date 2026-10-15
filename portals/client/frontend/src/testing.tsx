// What the component tests share: the browser APIs jsdom lacks, Relay's mock environment, and
// the providers a page renders inside. Imported by `*.test.tsx` only.
import { MantineProvider } from "@mantine/core";
import { cleanup, render } from "@testing-library/react";
import type { ReactNode } from "react";
import { RelayEnvironmentProvider } from "react-relay";
import { createMockEnvironment } from "relay-test-utils";
import { afterEach, vi } from "vitest";

// relay-test-utils' mock environment calls jest.fn; Vitest's vi is the same API.
Object.assign(globalThis, { jest: vi });

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

// Without Vitest globals, Testing Library doesn't unmount between tests by itself.
afterEach(cleanup);

/**
 * Renders `ui` as a page would be, against a mock Relay environment, which it returns: a fresh
 * one, or `environment` when a test has loaded the page's query into it first.
 */
export function renderWithRelay(ui: ReactNode, environment = createMockEnvironment()) {
  render(
    <MantineProvider>
      <RelayEnvironmentProvider environment={environment}>{ui}</RelayEnvironmentProvider>
    </MantineProvider>,
  );
  return environment;
}
