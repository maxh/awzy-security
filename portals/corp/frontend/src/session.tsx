// Corp portal sign-in: username and password, for demos and testing only (ADR 0025). The
// session is an HttpOnly cookie the backend sets at /auth/login; who is signed in comes from the
// GraphQL `viewer` query.

import { AuthLayout, isRecord, ShellAccount } from "@awzy/ui";
import { Alert, Button, PasswordInput, Stack, TextInput } from "@mantine/core";
import { createContext, useContext, useState, type FormEvent } from "react";

export type Staff = { username: string; name: string };

/** `v` as a staff member, if it is one. */
function asStaff(v: unknown): Staff | null {
  return isRecord(v) && typeof v.username === "string" && typeof v.name === "string"
    ? { username: v.username, name: v.name }
    : null;
}

export const StaffContext = createContext<Staff | null>(null);

/** Who is signed in, or null: `/graphql` answers 401 without a session. */
export async function currentStaff(): Promise<Staff | null> {
  const res = await fetch("/graphql", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ query: "{ viewer { username name } }" }),
  });
  if (!res.ok) return null;
  const body: unknown = await res.json();
  return isRecord(body) && isRecord(body.data) ? asStaff(body.data.viewer) : null;
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
      const signedIn = asStaff(await res.json());
      if (signedIn) onSignedIn(signedIn);
      else setError("The portal's answer wasn't a staff member.");
    } else {
      setError(await res.text());
    }
  };

  return (
    <AuthLayout
      product="Corp portal"
      title="Sign in"
      description="For awzy staff. Demo and testing accounts are set up by an admin."
    >
      <form onSubmit={submit}>
        <Stack>
          {error && (
            <Alert color="red" variant="light">
              {error}
            </Alert>
          )}
          <TextInput
            label="Username"
            autoComplete="username"
            value={username}
            onChange={(e) => setUsername(e.currentTarget.value)}
            required
            autoFocus
          />
          <PasswordInput
            label="Password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.currentTarget.value)}
            required
          />
          <Button type="submit" loading={busy} fullWidth mt="xs">
            Sign in
          </Button>
        </Stack>
      </form>
    </AuthLayout>
  );
}

/** The signed-in staff member and a sign-out menu, for the sidebar. */
export function Account() {
  const staff = useContext(StaffContext);
  if (!staff) return null;
  return <ShellAccount name={staff.name} detail={staff.username} onSignOut={() => void signOut()} />;
}
