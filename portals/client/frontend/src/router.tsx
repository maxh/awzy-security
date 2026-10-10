import { PageErrorBoundary, PortalShell, ShellNavLink } from "@awzy/ui";
import {
  IconAddressBook,
  IconFileCode,
  IconHandStop,
  IconListCheck,
  IconSearch,
  IconShieldCheck,
} from "@tabler/icons-react";
import {
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  useRouterState,
} from "@tanstack/react-router";

import { ChallengesPage } from "./ChallengesPage";
import { DecisionsPage } from "./DecisionsPage";
import { DirectoryPage } from "./DirectoryPage";
import { EnforcementPage } from "./EnforcementPage";
import { InvestigationsPage } from "./InvestigationsPage";
import { PoliciesPage } from "./PoliciesPage";
import { Account } from "./session";

function Nav() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const icon = { size: 18, stroke: 1.6 };
  return (
    <>
      <ShellNavLink
        label="Decisions"
        icon={<IconListCheck {...icon} />}
        active={pathname.startsWith("/decisions")}
        renderRoot={(props) => <Link to="/decisions" {...props} />}
      />
      <ShellNavLink
        label="Policies"
        icon={<IconFileCode {...icon} />}
        active={pathname.startsWith("/policies")}
        renderRoot={(props) => <Link to="/policies" {...props} />}
      />
      <ShellNavLink
        label="Enforcement"
        icon={<IconShieldCheck {...icon} />}
        active={pathname.startsWith("/enforcement")}
        renderRoot={(props) => <Link to="/enforcement" {...props} />}
      />
      <ShellNavLink
        label="Directory"
        icon={<IconAddressBook {...icon} />}
        active={pathname.startsWith("/directory")}
        renderRoot={(props) => <Link to="/directory" {...props} />}
      />
      <ShellNavLink
        label="Challenges"
        icon={<IconHandStop {...icon} />}
        active={pathname.startsWith("/challenges")}
        renderRoot={(props) => <Link to="/challenges" {...props} />}
      />
      <ShellNavLink
        label="Investigations"
        icon={<IconSearch {...icon} />}
        active={pathname.startsWith("/investigations")}
        renderRoot={(props) => <Link to="/investigations" {...props} />}
      />
    </>
  );
}

function Root() {
  // A page that fails to load shows its error inside the shell; navigating away starts fresh.
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return (
    <PortalShell product="Client portal" audience="client" nav={<Nav />} account={<Account />}>
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

const challengesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/challenges",
  component: ChallengesPage,
});

const investigationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/investigations",
  component: InvestigationsPage,
});

export const router = createRouter({
  routeTree: rootRoute.addChildren([
    indexRoute,
    decisionsRoute,
    policiesRoute,
    enforcementRoute,
    directoryRoute,
    challengesRoute,
    investigationsRoute,
  ]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
