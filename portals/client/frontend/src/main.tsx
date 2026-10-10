import "@mantine/core/styles.css";

import { createEnvironment } from "@awzy/relay";
import { AwzyProvider } from "@awzy/ui";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RelayEnvironmentProvider } from "react-relay";

import { signIn } from "./auth";
import { router } from "./router";
import { SessionContext } from "./session";

void signIn().then((session) => {
  const environment = createEnvironment("/graphql", { token: session.token, onUnauthorized: session.signInAgain });
  createRoot(document.getElementById("root")!).render(
    <StrictMode>
      <SessionContext.Provider value={session}>
        <RelayEnvironmentProvider environment={environment}>
          <AwzyProvider>
            <RouterProvider router={router} />
          </AwzyProvider>
        </RelayEnvironmentProvider>
      </SessionContext.Provider>
    </StrictMode>,
  );
});
