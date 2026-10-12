import { createLocalJWKSet, exportJWK, generateKeyPair, SignJWT, type CryptoKey } from "jose";
import { beforeAll, describe, expect, it } from "vitest";

import { TokenError, TokenVerifier } from "./token.ts";

let idpKey: CryptoKey;
let appKey: CryptoKey;
let idpKeys: ReturnType<typeof createLocalJWKSet>;
let appKeys: ReturnType<typeof createLocalJWKSet>;

beforeAll(async () => {
  const idp = await generateKeyPair("ES256");
  const app = await generateKeyPair("ES256");
  idpKey = idp.privateKey;
  appKey = app.privateKey;
  idpKeys = createLocalJWKSet({ keys: [{ ...(await exportJWK(idp.publicKey)), kid: "idp", alg: "ES256" }] });
  appKeys = createLocalJWKSet({ keys: [{ ...(await exportJWK(app.publicKey)), kid: "app", alg: "ES256" }] });
});

async function token(
  claims: Record<string, unknown>,
  { key = idpKey, kid = "idp", iss = "https://idp.test", aud = "awzy" } = {},
): Promise<string> {
  const jwt = await new SignJWT(claims)
    .setProtectedHeader({ alg: "ES256", kid })
    .setIssuer(iss)
    .setAudience(aud)
    .setExpirationTime("5m")
    .sign(key);
  return `Bearer ${jwt}`;
}

function verifier(extra: Partial<ConstructorParameters<typeof TokenVerifier>[0]> = {}): TokenVerifier {
  return new TokenVerifier({ issuer: "https://idp.test", audience: "awzy", keys: idpKeys, ...extra });
}

describe("TokenVerifier", () => {
  it("makes a person of a plain token and an agent acting for them of an act claim", async () => {
    const v = verifier();
    expect(await v.principal(await token({ sub: "alice" }))).toEqual({ principal: { type: "Human", id: "alice" } });
    expect(
      await v.principal(await token({ sub: "alice", act: { sub: "sales-genie" }, awzy_human_present: false })),
    ).toEqual({
      principal: { type: "ServiceAccount", id: "sales-genie" },
      onBehalfOf: { type: "Human", id: "alice" },
      humanPresent: false,
    });
  });

  it("carries the work item an agent's token names", async () => {
    const v = verifier();
    expect(
      await v.principal(await token({ sub: "alice", act: { sub: "sales-genie" }, awzy_work_item_id: "jira:SALES-12" })),
    ).toEqual({
      principal: { type: "ServiceAccount", id: "sales-genie" },
      onBehalfOf: { type: "Human", id: "alice" },
      workItemId: "jira:SALES-12",
    });
    const blank = await v.principal(await token({ sub: "alice", awzy_work_item_id: "  " }));
    expect(blank).toEqual({ principal: { type: "Human", id: "alice" } });
  });

  it("knows an application's own token by its client-credentials claims", async () => {
    const v = verifier({ subjectClaim: "email" });
    const apps = [
      { sub: "nightly-sync", client_id: "nightly-sync" },
      { sub: "0oa9app", cid: "0oa9app" },
      { sub: "5f1c-app", idtyp: "app" },
    ];
    const principals = await Promise.all(apps.map(async (claims) => v.principal(await token(claims))));
    expect(principals.map((p) => p.principal.type)).toEqual(["ServiceAccount", "ServiceAccount", "ServiceAccount"]);
    expect((await v.principal(await token({ sub: "00u1", email: "bob@acme.com" }))).principal).toEqual({
      type: "Human",
      id: "bob@acme.com",
    });
    await expect(v.principal(await token({ sub: "00u1" }))).rejects.toThrow(/no "email" claim/);
  });

  it("refuses a token for another audience, from another issuer, or missing", async () => {
    const v = verifier();
    await expect(v.principal(await token({ sub: "alice" }, { aud: "other" }))).rejects.toBeInstanceOf(TokenError);
    await expect(v.principal(await token({ sub: "alice" }, { iss: "https://evil.test" }))).rejects.toBeInstanceOf(
      TokenError,
    );
    await expect(v.principal(undefined)).rejects.toThrow("missing bearer token");
    await expect(v.principal("Bearer not-a-jwt")).rejects.toBeInstanceOf(TokenError);
  });

  it("takes an app's tokens only for its own agents", async () => {
    const v = verifier({
      trustedApps: [{ name: "acme-crm", issuer: "https://crm.acme.example", keys: appKeys, audience: "awzy-agents" }],
    });
    const crm = (claims: Record<string, unknown>) =>
      token(claims, { key: appKey, kid: "app", iss: "https://crm.acme.example", aud: "awzy-agents" });
    expect(await v.principal(await crm({ sub: "acme-crm.renewals-bot" }))).toEqual({
      principal: { type: "ServiceAccount", id: "acme-crm.renewals-bot" },
    });
    expect(await v.principal(await crm({ sub: "dana", act: { sub: "acme-crm.assistant" } }))).toEqual({
      principal: { type: "ServiceAccount", id: "acme-crm.assistant" },
      onBehalfOf: { type: "Human", id: "dana" },
    });
    const refused = [{ sub: "dana" }, { sub: "sales-genie" }, { sub: "acme-crm" }, { sub: "x", act: { sub: "bot" } }];
    await Promise.all(
      refused.map(async (claims) => expect(v.principal(await crm(claims))).rejects.toThrow(/signs tokens only for/)),
    );
    await expect(v.principal(await token({ sub: "acme-crm.bot" }, { key: appKey, kid: "app" }))).rejects.toBeInstanceOf(
      TokenError,
    );
  });

  it("refuses a JWKS URL that isn't https", () => {
    expect(() => verifier({ keys: "http://idp.test/jwks" })).toThrow(/https/);
  });
});
