// @vitest-environment jsdom
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { loadQuery } from "react-relay";
import type { OperationDescriptor } from "relay-runtime";
import { createMockEnvironment, MockPayloadGenerator, type RelayMockEnvironment } from "relay-test-utils";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import enforcementPageQuery, { type EnforcementPageQuery } from "./__generated__/EnforcementPageQuery.graphql";
import { EnforcementPage } from "./EnforcementPage";
import { passChallenge } from "./securityKeyChallenge";
import { renderWithRelay } from "./testing";

const page = vi.hoisted((): { queryRef: unknown; invalidate: () => Promise<void> } => ({
  queryRef: null,
  invalidate: () => Promise.resolve(),
}));

vi.mock("@tanstack/react-router", () => ({
  getRouteApi: () => ({ useLoaderData: () => page.queryRef }),
  useRouter: () => ({ invalidate: () => page.invalidate() }),
}));

vi.mock(import("./securityKeyChallenge"), async (importOriginal) => ({
  ...(await importOriginal()),
  passChallenge: vi.fn(() => Promise.resolve()),
}));

/**
 * Answers each operation the page sends, as it's sent, with `answer`: the page's query, the
 * sections' own queries, and each mutation. (The mock environment's own queued resolvers don't
 * see an operation `loadQuery` starts.)
 */
function answerEverything(
  environment: RelayMockEnvironment,
  answer: (operation: OperationDescriptor) => ReturnType<typeof MockPayloadGenerator.generate>,
) {
  const timer = setInterval(() => {
    for (const operation of environment.mock.getAllOperations()) {
      environment.mock.resolve(operation, answer(operation));
    }
  }, 5);
  onTestFinished(() => clearInterval(timer));
}

/**
 * The Enforcement page for someone policy lets switch (or not), in monitor mode. Each switch
 * answers with `answers` in turn: a challenge to pass first, or the switch made.
 */
function renderEnforcement({ canSwitch, answers }: { canSwitch: boolean; answers: ("challenge" | "switched")[] }) {
  const environment = createMockEnvironment();
  const switches: unknown[] = [];
  answerEverything(environment, (operation: OperationDescriptor) => {
    if (operation.request.node.params.name === "EnforcementPageSetModeMutation") {
      switches.push(operation.request.variables);
      const answer = answers[switches.length - 1];
      return MockPayloadGenerator.generate(operation, {
        SetEnforcementModePayload: () =>
          answer === "challenge"
            ? { challengeQid: "challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90", enforcement: null }
            : { challengeQid: null, enforcement: { mode: "ENFORCE" } },
      });
    }
    return MockPayloadGenerator.generate(operation, {
      Query: () => ({ canSwitch, enforcementScopes: [] }),
      OutageFallback: () => ({ tools: [], servers: [], failOpenTiers: [], changedAt: null, changedBy: null }),
      Enforcement: () => ({ mode: "MONITOR", changedAt: null, changedBy: null }),
      EnforcementSummary: () => ({ since: "2026-10-14T12:00:00Z", total: 100, monitored: 100 }),
      Decision: () => ({ at: "2026-10-15T09:00:00Z", policies: ["contractors-outside-their-work"] }),
    });
  });
  const invalidate = vi.fn(() => Promise.resolve());
  page.invalidate = invalidate;
  page.queryRef = loadQuery<EnforcementPageQuery>(environment, enforcementPageQuery, {});
  renderWithRelay(<EnforcementPage />, environment);
  return { switches, invalidate };
}

/** Asks to switch the install to enforce, and confirms. Its switch is the page's first. */
async function switchInstallToEnforce(user: ReturnType<typeof userEvent.setup>) {
  const [installSwitch] = await screen.findAllByRole("button", { name: "Switch to enforce" });
  if (!installSwitch) throw new Error("no switch to enforce");
  await user.click(installSwitch);
  await user.click(await screen.findByRole("button", { name: "Enforce" }));
}

describe("EnforcementPage", () => {
  it("switches the install to enforce only after the admin's security key passes the challenge", async () => {
    const user = userEvent.setup();
    vi.mocked(passChallenge).mockClear();
    const { switches, invalidate } = renderEnforcement({ canSwitch: true, answers: ["challenge", "switched"] });

    await switchInstallToEnforce(user);

    await vi.waitFor(() => expect(invalidate).toHaveBeenCalled());
    expect(passChallenge).toHaveBeenCalledWith(expect.anything(), "challenge_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90");
    expect(switches).toEqual([
      { input: { mode: "ENFORCE", scope: "INSTALL" } },
      { input: { mode: "ENFORCE", scope: "INSTALL" } },
    ]);
  });

  it("doesn't switch when the security key prompt is cancelled, and says why", async () => {
    const user = userEvent.setup();
    vi.mocked(passChallenge).mockRejectedValueOnce(new DOMException("cancelled", "NotAllowedError"));
    const { switches, invalidate } = renderEnforcement({ canSwitch: true, answers: ["challenge", "switched"] });

    await switchInstallToEnforce(user);

    expect(await screen.findByText(/security key prompt was cancelled/)).toBeTruthy();
    expect(switches).toHaveLength(1);
    expect(invalidate).not.toHaveBeenCalled();
  });

  it("says so when awzy still asks for a challenge after it was passed", async () => {
    const user = userEvent.setup();
    const { switches, invalidate } = renderEnforcement({ canSwitch: true, answers: ["challenge", "challenge"] });

    await switchInstallToEnforce(user);

    expect(await screen.findByText(/still asks for a challenge/)).toBeTruthy();
    expect(switches).toHaveLength(2);
    expect(invalidate).not.toHaveBeenCalled();
  });

  it("offers no switch to someone policy doesn't let switch", async () => {
    renderEnforcement({ canSwitch: false, answers: [] });

    expect(await screen.findByText(/You can't switch modes/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Switch to enforce" })).toBeNull();
  });
});
