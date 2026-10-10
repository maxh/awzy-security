import { PageErrorBoundary, PortalShell } from "@awzy/ui";
import {
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useRouterState,
} from "@tanstack/react-router";

import { DecisionsPage } from "./DecisionsPage";
import { DirectoryPage } from "./DirectoryPage";
import { EnforcementPage } from "./EnforcementPage";
import { PoliciesPage } from "./PoliciesPage";
import { Account } from "./session";

function Nav() {
  const linkProps = {
    style: { color: "inherit", textDecoration: "none" },
    activeProps: { style: { color: "inherit", textDecoration: "none", fontWeight: 600 } },
  };
  return (
    <>
      <Link to="/decisions" {...linkProps}>
        Decisions
      </Link>
      <Link to="/policies" {...linkProps}>
        Policies
      </Link>
      <Link to="/enforcement" {...linkProps}>
        Enforcement
      </Link>
      <Link to="/directory" {...linkProps}>
        Directory
      </Link>
    </>
  );
}

function Root() {
  // A page that fails to load shows its error inside the shell; navigating away starts fresh.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <PortalShell title="awzy client portal" audience="client" nav={<Nav />} account={<Account />}>
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
    throw redirect({ to: "/decisions" });
  },
});

const decisionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/decisions",
  component: DecisionsPage,
});

const policiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/policies",
  component: PoliciesPage,
});

const enforcementRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/enforcement",
  component: EnforcementPage,
});

const directoryRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/directory",
  component: DirectoryPage,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([indexRoute, decisionsRoute, policiesRoute, enforcementRoute, directoryRoute]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
