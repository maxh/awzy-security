import "@mantine/core/styles.css";

import { createEnvironment } from "@awzy/relay";
import { AwzyProvider } from "@awzy/ui";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { RelayEnvironmentProvider } from "react-relay";

import { router } from "./router";
import { currentStaff, SignInPage, StaffContext, type Staff } from "./session";

// The session cookie goes with every request; an expired one sends the viewer back to sign in.
const environment = createEnvironment("/graphql", {
  onUnauthorized: () => window.location.reload(),
  sendQueryText: import.meta.env.DEV,
});

function App({ initial }: { initial: Staff | null }) {
  const [staff, setStaff] = useState(initial);
  if (!staff) return <SignInPage onSignedIn={setStaff} />;
  return (
    <StaffContext.Provider value={staff}>
      <RelayEnvironmentProvider environment={environment}>
        <RouterProvider router={router} />
      </RelayEnvironmentProvider>
    </StaffContext.Provider>
  );
}

void currentStaff().then((staff) => {
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <AwzyProvider>
        <App initial={staff} />
      </AwzyProvider>
    </StrictMode>,
  );
});
