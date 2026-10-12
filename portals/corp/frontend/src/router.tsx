import { PageErrorBoundary, PortalShell, ShellNavLink } from "@awzy/ui";
import { IconLifebuoy, IconPackage, IconServer2 } from "@tabler/icons-react";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  lazyRouteComponent,
  Link,
  Outlet,
  redirect,
  useRouterState,
} from "@tanstack/react-router";

import type { RelayRouterContext } from "@awzy/relay/routes";

import { loadFleetPage, loadInstallPage, loadReleasesPage, loadSupportPage } from "./loaders";
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
      <ShellNavLink
        label="Support"
        icon={<IconLifebuoy size={18} stroke={1.6} />}
        active={pathname.startsWith("/support")}
        renderRoot={(props) => <Link to="/support" {...props} />}
      />
    </>
  );
}

function Root() {
  // A page that fails to load shows its error inside the shell; navigating away starts fresh.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <PortalShell
      product="Corp portal"
      audience="corp"
      renderHome={(props) => <Link to="/" {...props} />}
      nav={<Nav />}
      account={<Account />}
    >
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
  component: lazyRouteComponent(() => import("./FleetPage"), "FleetPage"),
});
const installRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/installs/$installId",
  loader: ({ context, params }) => loadInstallPage(context.environment, { id: params.installId }),
  component: lazyRouteComponent(() => import("./InstallPage"), "InstallPage"),
});
const releasesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/releases",
  loader: ({ context }) => loadReleasesPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./ReleasesPage"), "ReleasesPage"),
});

const supportRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/support",
  loader: ({ context }) => loadSupportPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./SupportPage"), "SupportPage"),
});

export const router = createRouter({
  // Hovering over a link starts the page's code and its query (ADR 0099).
  defaultPreload: "intent",
  routeTree: rootRoute.addChildren([indexRoute, installsRoute, installRoute, releasesRoute, supportRoute]),
  // Set where the router is rendered, from the signed-in environment.
  context: { environment: undefined! },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
