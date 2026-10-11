// Each page's main query, started by its route's loader as navigation begins, or on hover over a
// link to it (ADR 0099). Kept apart from the pages so the router can load each page's code lazily.
import { routeQuery } from "@awzy/relay/routes";

import type { FleetPageQuery } from "./__generated__/FleetPageQuery.graphql";
import FleetPageQueryNode from "./__generated__/FleetPageQuery.graphql";
import type { InstallPageQuery } from "./__generated__/InstallPageQuery.graphql";
import InstallPageQueryNode from "./__generated__/InstallPageQuery.graphql";
import type { ReleasesPageQuery } from "./__generated__/ReleasesPageQuery.graphql";
import ReleasesPageQueryNode from "./__generated__/ReleasesPageQuery.graphql";
import type { SupportPageQuery } from "./__generated__/SupportPageQuery.graphql";
import SupportPageQueryNode from "./__generated__/SupportPageQuery.graphql";

export const loadFleetPage = routeQuery<FleetPageQuery>(FleetPageQueryNode);
export const loadInstallPage = routeQuery<InstallPageQuery>(InstallPageQueryNode);
export const loadReleasesPage = routeQuery<ReleasesPageQuery>(ReleasesPageQueryNode);
export const loadSupportPage = routeQuery<SupportPageQuery>(SupportPageQueryNode);
