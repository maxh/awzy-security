import { UserManager, WebStorageStateStore } from "oidc-client-ts";

/** Where the IdP sends the browser back after sign-in. A frontend route, not an API route. */
const CALLBACK_PATH = "/auth/callback";

/**
 * Where awzy staff land with a support session's token (ADR 0095), as `#token=<token>`. The corp
 * portal links here.
 */
const SUPPORT_PATH = "/support/sign-in";
const SUPPORT_TOKEN = "awzy-support-token";

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
export async function signIn(): Promise<Session> {
  const support = supportSession();
  if (support) return support;
  const config = (await (await fetch("/auth/config")).json()) as { issuer: string; clientId: string };
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
    const user = await manager.signinRedirectCallback();
    const returnTo = (user.state as { returnTo?: string } | undefined)?.returnTo ?? "/";
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
    claims = JSON.parse(atob((token.split(".")[1] ?? "").replace(/-/g, "+").replace(/_/g, "/"))) as typeof claims;
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
