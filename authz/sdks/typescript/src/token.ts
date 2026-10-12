import { createRemoteJWKSet, decodeJwt, jwtVerify, type JWTPayload, type JWTVerifyGetKey } from "jose";

import type { Principal } from "./types.ts";

/** Where a token's signing keys come from: a JWKS URL (https only), or keys the app supplies. */
export type Keys = string | JWTVerifyGetKey;

export interface TokenOptions {
  /** The client's IdP: the `iss` its tokens carry. */
  issuer: string;
  /** The `aud` tokens for this app must carry. */
  audience: string;
  keys: Keys;
  /**
   * The claim naming a person: their user name as the IdP provisions them. Default `sub`; with
   * Okta or Entra usually `email`. A service account's own token keeps its `sub`.
   */
  subjectClaim?: string;
  /** Service accounts whose own tokens carry no client-credentials marker, by ID. */
  serviceAccounts?: string[];
  /** Apps trusted to sign tokens for their own agents (the Directory page's Apps tab). */
  trustedApps?: TrustedApp[];
}

/** An app that signs tokens for the service accounts whose ID starts with its name and a dot. */
export interface TrustedApp {
  name: string;
  issuer: string;
  keys: Keys;
  audience: string;
}

export class TokenError extends Error {
  override name = "TokenError";
}

const ALGORITHMS = ["RS256", "ES256"];

function keySet(keys: Keys): JWTVerifyGetKey {
  if (typeof keys !== "string") return keys;
  const url = new URL(keys);
  // Keys fetched over plain HTTP could be swapped in transit and used to forge tokens.
  if (url.protocol !== "https:") throw new TokenError(`a JWKS URL must be https://, not ${keys}`);
  return createRemoteJWKSet(url);
}

interface Claims extends JWTPayload {
  act?: { sub?: string };
  awzy_human_present?: boolean;
  awzy_work_item_id?: string;
}

const text = (claims: Claims, key: string): string | undefined => {
  const v = claims[key];
  return typeof v === "string" ? v : undefined;
};

/** A token an application got for itself: RFC 9068's `client_id`, Okta's `cid`, Entra's `idtyp`. */
function isClientCredentials(claims: Claims): boolean {
  return (
    text(claims, "client_id") === claims.sub || text(claims, "cid") === claims.sub || text(claims, "idtyp") === "app"
  );
}

/**
 * Validates callers' tokens (signature, issuer, audience, expiry) and builds the principal from
 * their claims only, as awzy-gateway does (ADR 0045, rule 1).
 */
export class TokenVerifier {
  readonly #options: TokenOptions;
  readonly #keys: JWTVerifyGetKey;
  readonly #apps: Map<string, { app: TrustedApp; keys: JWTVerifyGetKey }>;

  constructor(options: TokenOptions) {
    this.#options = options;
    this.#keys = keySet(options.keys);
    this.#apps = new Map((options.trustedApps ?? []).map((app) => [app.issuer, { app, keys: keySet(app.keys) }]));
  }

  /** The principal an `Authorization` header's bearer token names, or a {@link TokenError}. */
  async principal(authorization: string | undefined): Promise<Principal> {
    const token = authorization?.startsWith("Bearer ") ? authorization.slice("Bearer ".length) : undefined;
    if (!token) throw new TokenError("missing bearer token");
    let iss: string | undefined;
    try {
      iss = decodeJwt(token).iss;
    } catch {
      throw new TokenError("not a JWT");
    }
    const app = iss && iss !== this.#options.issuer ? this.#apps.get(iss) : undefined;
    if (app) return this.#appPrincipal(token, app.app, app.keys);
    const claims = await this.#verify(token, this.#keys, this.#options.issuer, this.#options.audience);
    const sub = claims.sub!;
    if (claims.act?.sub) {
      return this.#withAwzyClaims(claims, {
        principal: { type: "ServiceAccount", id: claims.act.sub },
        onBehalfOf: { type: "Human", id: this.#human(claims) },
      });
    }
    if (isClientCredentials(claims) || this.#options.serviceAccounts?.includes(sub)) {
      return this.#withAwzyClaims(claims, { principal: { type: "ServiceAccount", id: sub } });
    }
    return this.#withAwzyClaims(claims, { principal: { type: "Human", id: this.#human(claims) } });
  }

  async #verify(token: string, keys: JWTVerifyGetKey, issuer: string, audience: string): Promise<Claims> {
    try {
      const { payload } = await jwtVerify<Claims>(token, keys, {
        issuer,
        audience,
        algorithms: ALGORITHMS,
        requiredClaims: ["exp", "iss", "aud", "sub"],
      });
      return payload;
    } catch (e) {
      throw new TokenError(`invalid token: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  #human(claims: Claims): string {
    const claim = this.#options.subjectClaim;
    if (!claim || claim === "sub") return claims.sub!;
    const id = text(claims, claim);
    if (!id) throw new TokenError(`the token has no ${JSON.stringify(claim)} claim to identify the person`);
    return id;
  }

  /** An app signs only for its own agents, on their own or acting for a person: never a person. */
  async #appPrincipal(token: string, app: TrustedApp, keys: JWTVerifyGetKey): Promise<Principal> {
    const claims = await this.#verify(token, keys, app.issuer, app.audience);
    const agent = claims.act?.sub ?? claims.sub!;
    const prefix = `${app.name}.`;
    if (!agent.startsWith(prefix) || agent.length === prefix.length) {
      throw new TokenError(`${app.name} signs tokens only for service accounts under "${prefix}", not "${agent}"`);
    }
    return this.#withAwzyClaims(claims, {
      principal: { type: "ServiceAccount", id: agent },
      ...(claims.act?.sub ? { onBehalfOf: { type: "Human", id: claims.sub! } } : {}),
    });
  }

  /** The principal with what awzy's own claims add: whether a person is present, and the work item. */
  #withAwzyClaims(claims: Claims, p: Principal): Principal {
    const workItemId = typeof claims.awzy_work_item_id === "string" ? claims.awzy_work_item_id.trim() : "";
    return {
      ...p,
      ...(typeof claims.awzy_human_present === "boolean" ? { humanPresent: claims.awzy_human_present } : {}),
      ...(workItemId ? { workItemId } : {}),
    };
  }
}
