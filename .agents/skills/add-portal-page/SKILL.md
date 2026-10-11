---
name: add-portal-page
description: Add or change a page in the client or corp portal frontend. Use whenever a portal route, page component, or its GraphQL queries change.
---

# Adding a portal page

- **Data loads with the route (ADR 0099).** Add the page's main query to `src/loaders.ts` with
  `routeQuery`, call it from the route's `loader`, and read it in the page with
  `usePreloadedQuery` through `getRouteApi(path).useLoaderData()`. Queries the page runs on input
  (a search, a filter, a tab) stay lazy. Refresh with `router.invalidate()`, not a fetch key.
- **The route loads lazily.** Register it with `lazyRouteComponent(() => import("./XPage"),
  "XPage")`.
- **The nav knows its access.** Add it to `PAGES` in `Nav.tsx` with the `PortalArea` the page
  needs, so viewers who can't open it don't see it.
- **No `as`.** Narrow with `isOneOf`, `oneOf`, `isRecord`, or `messageOf` from `@awzy/ui`, or a
  type guard; oxlint enforces it.
- **Sensitive fields** can come back null when a policy hides them: show `<Hidden />`.
- **A mutation that can be challenged** returns `challengeQid`: pass it with `passChallenge`, then
  run the mutation again.
- **Phone width** works: tables scroll inside `Table.ScrollContainer`, and forms wrap.
