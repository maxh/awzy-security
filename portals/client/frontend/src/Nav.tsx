import { routeQuery } from "@awzy/relay/routes";
import { EmptyState, PageLoader, ShellNavLink } from "@awzy/ui";
import {
  IconAddressBook,
  IconFileCode,
  IconHandStop,
  IconHeartbeat,
  IconLifebuoy,
  IconListCheck,
  IconLock,
  IconPlugConnected,
  IconSearch,
  IconShieldCheck,
  IconUserSearch,
} from "@tabler/icons-react";
import { getRouteApi, Link, Navigate, useRouterState } from "@tanstack/react-router";
import { type ComponentType, Suspense } from "react";
import { graphql, usePreloadedQuery } from "react-relay";

import type { NavQuery, PortalArea } from "./__generated__/NavQuery.graphql";
import NavQueryNode from "./__generated__/NavQuery.graphql";

const query = graphql`
  query NavQuery {
    viewerAreas
  }
`;

/** Started by the root route's loader, once: which areas the viewer may view. */
export const loadNav = routeQuery<NavQuery>(NavQueryNode);

const root = getRouteApi("__root__");

type Page = {
  to:
    | "/decisions"
    | "/policies"
    | "/enforcement"
    | "/directory"
    | "/challenges"
    | "/investigations"
    | "/lookup"
    | "/connectors"
    | "/status"
    | "/support";
  label: string;
  icon: ComponentType<{ size?: number; stroke?: number }>;
  /** The area the viewer must be able to view; the page's queries ask for the same. */
  area: PortalArea;
};

const PAGES: Page[] = [
  { to: "/decisions", label: "Decisions", icon: IconListCheck, area: "DECISIONS" },
  { to: "/policies", label: "Policies", icon: IconFileCode, area: "POLICIES" },
  { to: "/enforcement", label: "Enforcement", icon: IconShieldCheck, area: "ENFORCEMENT" },
  { to: "/directory", label: "Directory", icon: IconAddressBook, area: "DIRECTORY" },
  { to: "/challenges", label: "Challenges", icon: IconHandStop, area: "CHALLENGES" },
  { to: "/investigations", label: "Investigations", icon: IconSearch, area: "INVESTIGATIONS" },
  { to: "/lookup", label: "Look up", icon: IconUserSearch, area: "DIRECTORY" },
  { to: "/connectors", label: "Connectors", icon: IconPlugConnected, area: "ENFORCEMENT" },
  { to: "/status", label: "Status", icon: IconHeartbeat, area: "ENFORCEMENT" },
  { to: "/support", label: "Support", icon: IconLifebuoy, area: "ENFORCEMENT" },
];

/** The pages the viewer may open. */
function usePages(): Page[] {
  const { viewerAreas } = usePreloadedQuery<NavQuery>(query, root.useLoaderData());
  return PAGES.filter((p) => viewerAreas.includes(p.area));
}

/** Links to the pages the viewer may open, and no others. */
export function Nav() {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  return usePages().map(({ to, label, icon: Icon }) => (
    <ShellNavLink
      key={to}
      label={label}
      icon={<Icon size={18} stroke={1.6} />}
      active={pathname.startsWith(to)}
      renderRoot={(props) => <Link to={to} {...props} />}
    />
  ));
}

/** `/`: the first page the viewer may open. */
export function Home() {
  return (
    <Suspense fallback={<PageLoader />}>
      <FirstPage />
    </Suspense>
  );
}

function FirstPage() {
  const [first] = usePages();
  if (!first) {
    return (
      <EmptyState
        icon={<IconLock size={22} />}
        title="Nothing here for you yet"
        description="Your access doesn't include any part of this portal. Ask your security team if you need it."
      />
    );
  }
  return <Navigate to={first.to} replace />;
}
