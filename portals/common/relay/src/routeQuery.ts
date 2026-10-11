import { loadQuery, type PreloadedQuery } from "react-relay";
import type {
  CacheConfig,
  FetchPolicy,
  GraphQLTaggedNode,
  IEnvironment,
  OperationType,
  VariablesOf,
} from "relay-runtime";

/** What every route's loader is given: the Relay environment its page's query loads into. */
export interface RelayRouterContext {
  environment: IEnvironment;
}

/**
 * A route's query, started by its loader as navigation begins rather than after the page renders
 * (ADR 0099). The page reads it with `usePreloadedQuery`.
 *
 * Each load retains its result in the store until disposed. A route keeps its last two: the
 * router can still render the previous one while the next one replaces it, and a mounted page
 * retains what it shows by itself, so anything older is safe to release.
 */
export function routeQuery<T extends OperationType>(
  query: GraphQLTaggedNode,
  fetchPolicy: FetchPolicy = "store-and-network",
) {
  let previous: PreloadedQuery<T> | null = null;
  let current: PreloadedQuery<T> | null = null;
  return (
    environment: IEnvironment,
    variables: VariablesOf<T>,
    networkCacheConfig?: CacheConfig,
  ): PreloadedQuery<T> => {
    previous?.dispose();
    previous = current;
    current = loadQuery<T>(environment, query, variables, { fetchPolicy, networkCacheConfig });
    return current;
  };
}
