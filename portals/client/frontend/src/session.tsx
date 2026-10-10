import { ShellAccount } from "@awzy/ui";
import { createContext, useContext } from "react";

import type { Session } from "./auth";

export const SessionContext = createContext<Session | null>(null);

/** The signed-in viewer and a sign-out menu, for the sidebar. */
export function Account() {
  const session = useContext(SessionContext);
  if (!session) return null;
  const profile = session.user.profile;
  const name = profile.name ?? profile.sub;
  return (
    <ShellAccount
      name={name}
      detail={profile.email && profile.email !== name ? profile.email : undefined}
      onSignOut={() => void session.signOut()}
    />
  );
}
