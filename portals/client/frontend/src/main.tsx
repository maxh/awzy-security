import "@mantine/core/styles.css";

import { createEnvironment } from "@awzy/relay";
import { AwzyProvider } from "@awzy/ui";
import { Alert, Button, Container } from "@mantine/core";
import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { RelayEnvironmentProvider } from "react-relay";

import { type Session, signIn } from "./auth";
import { router } from "./router";
import { SessionContext } from "./session";

const root = createRoot(document.getElementById("root")!);

function renderPortal(session: Session) {
  const environment = createEnvironment("/graphql", {
    token: session.token,
    onUnauthorized: session.signInAgain,
    sendQueryText: import.meta.env.DEV,
  });
  root.render(
    <StrictMode>
      <SessionContext.Provider value={session}>
        <RelayEnvironmentProvider environment={environment}>
          <AwzyProvider>
            <RouterProvider router={router} context={{ environment }} />
          </AwzyProvider>
        </RelayEnvironmentProvider>
      </SessionContext.Provider>
    </StrictMode>,
  );
}

/** Sign-in failed (the IdP refused, or its answer didn't check out): say so, not a blank page. */
function renderSignInFailed(error: unknown) {
  root.render(
    <StrictMode>
      <AwzyProvider>
        <Container size="xs" py="xl">
          <Alert color="red" title="Sign-in didn't complete">
            {error instanceof Error ? error.message : String(error)}
          </Alert>
          <Button mt="md" onClick={() => window.location.assign("/")}>
            Sign in again
          </Button>
        </Container>
      </AwzyProvider>
    </StrictMode>,
  );
}

void signIn().then(renderPortal, renderSignInFailed);
