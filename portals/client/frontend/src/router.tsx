import { PageErrorBoundary, PortalShell } from "@awzy/ui";
import { Suspense } from "react";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  lazyRouteComponent,
  Outlet,
  useRouterState,
} from "@tanstack/react-router";
import type { RelayRouterContext } from "@awzy/relay/routes";

import { SupportBanner, SupportPage } from "./SupportPage";
import { Home, loadNav, Nav } from "./Nav";
import {
  loadChallengesPage,
  loadConnectorsPage,
  loadDirectoryPageSummary,
  loadEnforcementPage,
  loadPoliciesPage,
  loadStatusPage,
  loadSupportPage,
} from "./loaders";
import { Account } from "./session";

function Root() {
  // A page that fails to load shows its error inside the shell; navigating away starts fresh.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <PortalShell
      product="Client portal"
      audience="client"
      nav={
        <Suspense fallback={null}>
          <Nav />
        </Suspense>
      }
      account={<Account />}
    >
      <SupportBanner />
      <PageErrorBoundary key={pathname}>
        <Outlet />
      </PageErrorBoundary>
    </PortalShell>
  );
}

const rootRoute = createRootRouteWithContext<RelayRouterContext>()({
  // What the viewer may view, once per sign-in: the nav and `/` follow it.
  loader: ({ context }) => loadNav(context.environment, {}),
  staleTime: Infinity,
  component: Root,
});

// A page's main query starts in its route's loader, as navigation begins (ADR 0099). Queries a
// page runs on input (a search, a filter, a tab) stay in the page.
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: Home,
});

const decisionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/decisions",
  component: lazyRouteComponent(() => import("./DecisionsPage"), "DecisionsPage"),
});

const policiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/policies",
  loader: ({ context }) => loadPoliciesPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./PoliciesPage"), "PoliciesPage"),
});

const enforcementRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/enforcement",
  loader: ({ context }) => loadEnforcementPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./EnforcementPage"), "EnforcementPage"),
});

const directoryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/directory",
  loader: ({ context }) => loadDirectoryPageSummary(context.environment, {}),
  component: lazyRouteComponent(() => import("./DirectoryPage"), "DirectoryPage"),
});

const challengesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/challenges",
  loader: ({ context }) => loadChallengesPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./ChallengesPage"), "ChallengesPage"),
});

const investigationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/investigations",
  component: lazyRouteComponent(() => import("./InvestigationsPage"), "InvestigationsPage"),
});

const lookupRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/lookup",
  component: lazyRouteComponent(() => import("./LookupPage"), "LookupPage"),
});

const connectorsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/connectors",
  loader: ({ context }) => loadConnectorsPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./ConnectorsPage"), "ConnectorsPage"),
});

const statusRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/status",
  loader: ({ context }) => loadStatusPage(context.environment, {}),
  component: lazyRouteComponent(() => import("./StatusPage"), "StatusPage"),
});

const supportRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/support",
  loader: ({ context }) => loadSupportPage(context.environment, {}),
  component: SupportPage,
});

export const router = createRouter({
  // Hovering over a link starts the page's code and its query (ADR 0099).
  defaultPreload: "intent",
  routeTree: rootRoute.addChildren([
    indexRoute,
    decisionsRoute,
    policiesRoute,
    enforcementRoute,
    directoryRoute,
    challengesRoute,
    investigationsRoute,
    lookupRoute,
    connectorsRoute,
    statusRoute,
    supportRoute,
  ]),
  // Set where the router is rendered, from the signed-in environment.
  context: { environment: undefined! },
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
