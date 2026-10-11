import { PageErrorBoundary, PortalShell, ShellNavLink } from "@awzy/ui";
import { IconPackage, IconServer2 } from "@tabler/icons-react";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useRouterState,
} from "@tanstack/react-router";

import type { RelayRouterContext } from "@awzy/relay/routes";

import { FleetPage, loadFleetPage } from "./FleetPage";
import { InstallPage, loadInstallPage } from "./InstallPage";
import { loadReleasesPage, ReleasesPage } from "./ReleasesPage";
import { Account } from "./session";

function Nav() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <>
      <ShellNavLink
        label="Installs"
        icon={<IconServer2 size={18} stroke={1.6} />}
        active={pathname.startsWith("/installs")}
        renderRoot={(props) => <Link to="/installs" {...props} />}
      />
      <ShellNavLink
        label="Releases"
        icon={<IconPackage size={18} stroke={1.6} />}
        active={pathname.startsWith("/releases")}
        renderRoot={(props) => <Link to="/releases" {...props} />}
      />
    </>
  );
}

function Root() {
  // A page that fails to load shows its error inside the shell; navigating away starts fresh.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <PortalShell product="Corp portal" audience="corp" nav={<Nav />} account={<Account />}>
      <PageErrorBoundary key={pathname}>
        <Outlet />
      </PageErrorBoundary>
    </PortalShell>
  );
}

const rootRoute = createRootRouteWithContext<RelayRouterContext>()({ component: Root });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/installs" });
  },
});

// Each page's query starts in its route's loader, as navigation begins (ADR 0099).
const installsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/installs",
  loader: ({ context }) => loadFleetPage(context.environment, {}),
  component: FleetPage,
});
const installRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/installs/$installId",
  loader: ({ context, params }) => loadInstallPage(context.environment, { id: params.installId }),
  component: InstallPage,
});
const releasesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/releases",
  loader: ({ context }) => loadReleasesPage(context.environment, {}),
  component: ReleasesPage,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([indexRoute, installsRoute, installRoute, releasesRoute]),
  // Set where the router is rendered, from the signed-in environment.
  context: { environment: undefined! },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
