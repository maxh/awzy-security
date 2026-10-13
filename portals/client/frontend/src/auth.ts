import { isRecord } from "@awzy/ui";
import { UserManager, WebStorageStateStore } from "oidc-client-ts";

/** Where the IdP sends the browser back after sign-in. A frontend route, not an API route. */
const CALLBACK_PATH = "/auth/callback";

/**
 * Where awzy staff land with a support session's token (ADR 0095), as `#token=<token>`. The corp
 * portal links here.
 */
const SUPPORT_PATH = "/support/sign-in";
const SUPPORT_TOKEN = "awzy-support-token";

/** Set while a sign-in started over after a stale callback, so a second failure stops there. */
const RETRIED_SIGN_IN = "awzy-sign-in-retried";

export type Session = {
  /** Who is signed in, for the sidebar. */
  profile: { sub: string; name?: string; email?: string };
  /** The access token for the portal backend, or null once it has expired. */
  token: () => string | null;
  /** Starts sign-in again, e.g. when the backend rejects an expired token. */
  signInAgain: () => void;
  signOut: () => Promise<void>;
};

/**
 * Signs the viewer in with the client's IdP: OIDC authorization code with PKCE. The backend
 * says which IdP at /auth/config. Resolves once there is a signed-in user; otherwise the browser
 * is sent to the IdP and the promise never resolves.
 */
/** The portal's `/auth/config`, checked: which IdP signs people in. */
function authConfig(v: unknown): { issuer: string; clientId: string } {
  if (isRecord(v) && typeof v.issuer === "string" && typeof v.clientId === "string") {
    return { issuer: v.issuer, clientId: v.clientId };
  }
  throw new Error("The portal's sign-in settings (/auth/config) aren't what this page expects.");
}

export async function signIn(): Promise<Session> {
  const support = supportSession();
  if (support) return support;
  const config = authConfig(await (await fetch("/auth/config")).json());
  const manager = new UserManager({
    authority: config.issuer,
    // The deployed IdP client only allows the portal's own redirect URI. Running this frontend
    // locally against a deployed backend needs a separate client that allows
    // http://localhost:5173/auth/callback (see vite.config.ts).
    client_id: import.meta.env.VITE_AWZY_CLIENT_ID ?? config.clientId,
    redirect_uri: window.location.origin + CALLBACK_PATH,
    post_logout_redirect_uri: window.location.origin,
    scope: "openid profile",
    loadUserInfo: false,
    userStore: new WebStorageStateStore({ store: window.sessionStorage }),
  });
  const signInAgain = () => {
    void manager.signinRedirect({ state: { returnTo: window.location.pathname + window.location.search } });
  };

  if (window.location.pathname === CALLBACK_PATH) {
    let user;
    try {
      user = await manager.signinRedirectCallback();
    } catch (error) {
      // A callback whose sign-in this tab already finished or never started: the back button to
      // the IdP's page, then signing in (as someone else) from it. Its state is gone, so the
      // code can't be trusted; start a fresh sign-in, once, rather than fail.
      window.history.replaceState(null, "", "/");
      if (window.sessionStorage.getItem(RETRIED_SIGN_IN) === null) {
        window.sessionStorage.setItem(RETRIED_SIGN_IN, "1");
        signInAgain();
        return new Promise(() => {});
      }
      window.sessionStorage.removeItem(RETRIED_SIGN_IN);
      throw error;
    }
    window.sessionStorage.removeItem(RETRIED_SIGN_IN);
    const state: unknown = user.state;
    const returnTo = isRecord(state) && typeof state.returnTo === "string" ? state.returnTo : "/";
    window.history.replaceState(null, "", returnTo.startsWith("/") ? returnTo : "/");
  }

  const user = await manager.getUser();
  if (!user || user.expired) {
    signInAgain();
    return new Promise(() => {});
  }
  return {
    profile: user.profile,
    token: () => (user.expired ? null : user.access_token),
    signInAgain,
    signOut: async () => {
      await manager.removeUser();
      try {
        await manager.signoutRedirect();
      } catch {
        // The IdP has no sign-out endpoint (the development IdP doesn't): sign in again instead.
        signInAgain();
      }
    },
  };
}

/**
 * An awzy staff member in a support session (ADR 0095): the corp portal's token, kept for this
 * tab only. The backend decides what it may see; the client's IdP isn't involved.
 */
function supportSession(): Session | null {
  if (window.location.pathname === SUPPORT_PATH) {
    const token = new URLSearchParams(window.location.hash.slice(1)).get("token");
    if (token) window.sessionStorage.setItem(SUPPORT_TOKEN, token);
    window.history.replaceState(null, "", "/");
  }
  const token = window.sessionStorage.getItem(SUPPORT_TOKEN);
  if (!token) return null;
  let claims: { sub?: string; exp?: number } = {};
  try {
    const parsed: unknown = JSON.parse(atob((token.split(".")[1] ?? "").replace(/-/g, "+").replace(/_/g, "/")));
    if (isRecord(parsed)) {
      claims = {
        sub: typeof parsed.sub === "string" ? parsed.sub : undefined,
        exp: typeof parsed.exp === "number" ? parsed.exp : undefined,
      };
    }
  } catch {
    // Not a token: the backend refuses it and the page says so.
  }
  const end = () => {
    window.sessionStorage.removeItem(SUPPORT_TOKEN);
    window.location.assign("/");
  };
  return {
    profile: { sub: claims.sub ?? "awzy support", name: `${claims.sub ?? "awzy"} (awzy support)` },
    token: () => ((claims.exp ?? 0) * 1000 > Date.now() ? token : null),
    // The session ended or expired: there's nothing to sign in to again.
    signInAgain: end,
    signOut: async () => end(),
  };
}
