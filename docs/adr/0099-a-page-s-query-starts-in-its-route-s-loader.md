# 0099. A page's query starts in its route's loader

- Status: Accepted
- Date: 2026-10-11

## Context

Every portal page fetched its data with Relay's `useLazyLoadQuery`, so a page's query started only
once the page rendered. Navigating meant loading the route, rendering the page, and only then
asking the backend, one step after another. A page refreshed after a mutation by bumping a
`fetchKey` that each page threaded through its components by hand.

Relay's answer is to start the query when navigation starts (`loadQuery`) and read it in the page
(`usePreloadedQuery`). [react-router-relay](https://github.com/loop-payments/react-router-relay)
does this for React Router's data routers, but both portals use TanStack Router, and switching
routers would rewrite both portals' routing for a feature TanStack Router already has: a route
`loader`, which runs as navigation starts. The platform plan's frontend stack already called for
route loaders that call `loadQuery`; the portals hadn't done it.

## Decision

- **A page's main query starts in its route's `loader`.** Each page exports a loader made with
  `routeQuery` (from `@awzy/relay/routes`), the route calls it with the Relay environment from the
  router's context, and the page reads the result with `usePreloadedQuery` through
  `getRouteApi(path).useLoaderData()`.
- **Queries a page runs on input stay in the page**, with `useLazyLoadQuery`: a search, a filter,
  a tab that mounts on demand, or a section only some viewers see. Their variables aren't known
  when navigation starts.
- **A page refreshes with `router.invalidate()`**, which runs the loaders again. With
  `store-and-network` the page keeps showing what it has while the new answer arrives.
- **Each route keeps at most its last two loads retained** in Relay's store; older ones are
  disposed. A mounted page retains what it shows by itself, so disposing earlier loads doesn't
  take data from under it, and the previous load stays usable while the router swaps in the next.
- **No new dependency.** react-router-relay isn't used.

## Consequences

- A page's data and its code load together, and a refresh after a mutation is one call.
- Two ways to fetch remain (preloaded for a page's main query, lazy for input-driven ones), and
  the rule above says which to use.
- Not yet done: preloading on hover (`defaultPreload: "intent"`), and code-splitting each page
  with `lazyRouteComponent`, which together would start a page's code and data before the click.
  The built bundles are large enough that Vite already warns.
