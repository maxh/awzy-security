// Each page's main query, started by its route's loader as navigation begins, or on hover over a
// link to it (ADR 0099). Kept apart from the pages so the router can load each page's code lazily.
import { routeQuery } from "@awzy/relay/routes";

import type { ChallengesPageQuery } from "./__generated__/ChallengesPageQuery.graphql";
import ChallengesPageQueryNode from "./__generated__/ChallengesPageQuery.graphql";
import type { ConnectorsPageQuery } from "./__generated__/ConnectorsPageQuery.graphql";
import ConnectorsPageQueryNode from "./__generated__/ConnectorsPageQuery.graphql";
import type { DirectoryPageSummaryQuery } from "./__generated__/DirectoryPageSummaryQuery.graphql";
import DirectoryPageSummaryQueryNode from "./__generated__/DirectoryPageSummaryQuery.graphql";
import type { EnforcementPageQuery } from "./__generated__/EnforcementPageQuery.graphql";
import EnforcementPageQueryNode from "./__generated__/EnforcementPageQuery.graphql";
import type { PoliciesPageQuery } from "./__generated__/PoliciesPageQuery.graphql";
import PoliciesPageQueryNode from "./__generated__/PoliciesPageQuery.graphql";
import type { StatusPageQuery } from "./__generated__/StatusPageQuery.graphql";
import StatusPageQueryNode from "./__generated__/StatusPageQuery.graphql";
import type { SupportPageQuery } from "./__generated__/SupportPageQuery.graphql";
import SupportPageQueryNode from "./__generated__/SupportPageQuery.graphql";

export const loadChallengesPage = routeQuery<ChallengesPageQuery>(ChallengesPageQueryNode);
export const loadConnectorsPage = routeQuery<ConnectorsPageQuery>(ConnectorsPageQueryNode, "network-only");
export const loadDirectoryPageSummary = routeQuery<DirectoryPageSummaryQuery>(DirectoryPageSummaryQueryNode);
export const loadEnforcementPage = routeQuery<EnforcementPageQuery>(EnforcementPageQueryNode);
export const loadPoliciesPage = routeQuery<PoliciesPageQuery>(PoliciesPageQueryNode);
export const loadStatusPage = routeQuery<StatusPageQuery>(StatusPageQueryNode, "network-only");
export const loadSupportPage = routeQuery<SupportPageQuery>(SupportPageQueryNode, "network-only");
