// Corp portal sign-in: username and password, for demos and testing only (ADR 0025). The
// session is an HttpOnly cookie the backend sets at /auth/login; who is signed in comes from the
// GraphQL `viewer` query.

import { Alert, Button, Card, Center, PasswordInput, Stack, Text, TextInput, Title } from "@mantine/core";
import { createContext, useContext, useState, type FormEvent } from "react";

export type Staff = { username: string; name: string };

export const StaffContext = createContext<Staff | null>(null);

/** Who is signed in, or null: `/graphql` answers 401 without a session. */
export async function currentStaff(): Promise<Staff | null> {
  const res = await fetch("/graphql", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: "{ viewer { username name } }" }),
  });
  if (!res.ok) return null;
  const body = (await res.json()) as { data?: { viewer?: Staff } };
  return body.data?.viewer ?? null;
}

export async function signOut(): Promise<void> {
  await fetch("/auth/logout", { method: "POST" });
  window.location.reload();
}

export function SignInPage({ onSignedIn }: { onSignedIn: (staff: Staff) => void }) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    const res = await fetch("/auth/login", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username, password }),
    });
    setBusy(false);
    if (res.ok) {
      onSignedIn((await res.json()) as Staff);
    } else {
      setError(await res.text());
    }
  };

  return (
    <Center h="100vh">
      <Card withBorder w={360} padding="lg">
        <form onSubmit={submit}>
          <Stack>
            <Title order={3}>awzy corp portal</Title>
            <Text size="sm" c="dimmed">
              Demo and testing sign-in. Accounts are set up by an admin.
            </Text>
            {error && <Alert color="red">{error}</Alert>}
            <TextInput
              label="Username"
              autoComplete="username"
              value={username}
              onChange={(e) => setUsername(e.currentTarget.value)}
              required
            />
            <PasswordInput
              label="Password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.currentTarget.value)}
              required
            />
            <Button type="submit" loading={busy}>
              Sign in
            </Button>
          </Stack>
        </form>
      </Card>
    </Center>
  );
}

/** The signed-in staff member and a sign-out button, for the header. */
export function Account() {
  const staff = useContext(StaffContext);
  if (!staff) return null;
  return (
    <>
      <Text size="sm">{staff.name}</Text>
      <Button size="xs" variant="subtle" onClick={() => void signOut()}>
        Sign out
      </Button>
    </>
  );
}
