import { UserManager, WebStorageStateStore, type User } from "oidc-client-ts";

/** Where the IdP sends the browser back after sign-in. A frontend route, not an API route. */
const CALLBACK_PATH = "/auth/callback";

export type Session = {
  user: User;
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
    user,
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
