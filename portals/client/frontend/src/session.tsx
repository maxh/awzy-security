import { Button, Text } from "@mantine/core";
import { createContext, useContext } from "react";

import type { Session } from "./auth";

export const SessionContext = createContext<Session | null>(null);

/** The signed-in viewer's name and a sign-out button, for the header. */
export function Account() {
  const session = useContext(SessionContext);
  if (!session) return null;
  const profile = session.user.profile;
  return (
    <>
      <Text size="sm">{profile.name ?? profile.sub}</Text>
      <Button size="xs" variant="subtle" onClick={() => void session.signOut()}>
        Sign out
      </Button>
    </>
  );
}
