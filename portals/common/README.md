# Portal common packages

TypeScript both portal frontends use. Shared code lives here so the two apps look and behave
alike; the backends share Rust through the top-level `libs/` instead. Part of the
[portals](../README.md).

| Package | For |
| --- | --- |
| `relay/` (`@awzy/relay`) | `createEnvironment`: a Relay environment for a portal backend, with queries and mutations over HTTP and subscriptions over graphql-ws, sending the viewer's token; and from `@awzy/relay/routes`, `routeQuery`, which starts a page's query in its route's loader (ADR 0099) |
| `ui/` (`@awzy/ui`) | The Mantine theme (`AwzyProvider`), `PortalShell` (header and navigation), `KangarooLogo` (the mark, also at `docs/brand/kangaroo.svg`), `PageErrorBoundary`, and `LineChart` |

Nothing here may hold client data or know which portal it's in: the corp portal uses it too.
