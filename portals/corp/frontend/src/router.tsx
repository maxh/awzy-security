import { PageErrorBoundary, PortalShell, ShellNavLink } from "@awzy/ui";
import { IconPackage, IconServer2 } from "@tabler/icons-react";
import {
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useRouterState,
} from "@tanstack/react-router";

import { FleetPage } from "./FleetPage";
import { InstallPage } from "./InstallPage";
import { ReleasesPage } from "./ReleasesPage";
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

const rootRoute = createRootRoute({ component: Root });

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/installs" });
  },
});

const installsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/installs", component: FleetPage });
const installRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/installs/$installId",
  component: InstallPage,
});
const releasesRoute = createRoute({ getParentRoute: () => rootRoute, path: "/releases", component: ReleasesPage });

export const router = createRouter({
  routeTree: rootRoute.addChildren([indexRoute, installsRoute, installRoute, releasesRoute]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
