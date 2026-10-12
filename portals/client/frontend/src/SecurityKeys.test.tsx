// @vitest-environment jsdom
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MockPayloadGenerator } from "relay-test-utils";
import { describe, expect, it, vi } from "vitest";

import { SecurityKeys } from "./SecurityKeys";
import { renderWithRelay } from "./testing";

const assertion = {
  ceremonyQid: "ceremony_00000000-0000-7000-8000-000000000001",
  credentialId: "cred",
  clientDataJson: "data",
  authenticatorData: "auth",
  signature: "sig",
};

// The browser's security key prompt, answered.
vi.mock("./webauthn", () => ({
  getAssertion: vi.fn(() => Promise.resolve(assertion)),
  createCredential: vi.fn(),
  webAuthnAvailable: () => true,
  webAuthnMessage: (e: unknown) => (e instanceof Error ? e.message : String(e)),
}));

const KEY = {
  id: "security_key_00000000-0000-7000-8000-000000000001",
  name: "YubiKey",
  createdAt: "2026-10-11T12:00:00Z",
  lastUsedAt: null,
};

describe("SecurityKeys", () => {
  it("removes a key only with a touch of a key the person has", async () => {
    const user = userEvent.setup();
    const onChanged = vi.fn();
    const environment = renderWithRelay(<SecurityKeys keys={[KEY]} onChanged={onChanged} />);
    expect(screen.getByText("never used")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Remove" }));
    const begin = environment.mock.getMostRecentOperation();
    expect(begin.request.node.params.name).toBe("SecurityKeysBeginKeyChangeMutation");
    environment.mock.resolve(
      begin,
      MockPayloadGenerator.generate(begin, {
        WebAuthnCeremony: () => ({ ceremonyQid: assertion.ceremonyQid, optionsJson: "{}" }),
      }),
    );

    await vi.waitFor(() =>
      expect(environment.mock.getMostRecentOperation().request.node.params.name).toBe("SecurityKeysRemoveKeyMutation"),
    );
    const remove = environment.mock.getMostRecentOperation();
    expect(remove.request.variables).toEqual({ input: { securityKeyQid: KEY.id, authorization: assertion } });
    environment.mock.resolve(remove, MockPayloadGenerator.generate(remove));
    await vi.waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("asks for no touch to add the first key", () => {
    renderWithRelay(<SecurityKeys keys={[]} onChanged={() => {}} />);
    expect(screen.getByText(/Add your first one here/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Remove" })).toBeNull();
  });
});
