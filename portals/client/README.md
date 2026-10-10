# Client portal · awzy-client-portal

The client's view of awzy, installed in their cluster with the rest of the install. Part of the
[portals](../README.md).

| Page | Shows |
| --- | --- |
| Decisions | Every PDP decision from ClickHouse, with the policies behind it |
| Policies | The policy bundles in awzy-policy |
| Enforcement | Each scope's mode (monitor or enforce) |
| Directory | People, groups, and service accounts from awzy-context |

- **`backend/`** (`awzy-client-portal`): GraphQL at `/graphql` (Relay server spec), an MCP
  server at `/mcp` so the client's own AI agents can use the portal, Awzy Bot (the Policies
  page's assistant, on the install's own Bedrock; off by default, ADR 0048), and the built
  frontend.
  Policies come from awzy-policy over gRPC, decisions from ClickHouse.
- **`frontend/`** (`@awzy/client-portal`): the React app. `schema.graphql` is generated from the
  backend; `bazel test` fails when it or the Relay artifacts are stale.
- **`e2e/`**: signs in through the dev IdP in headless Chrome and checks who the backend thinks
  is signed in (`//portals/client/e2e:signin_test`).

People sign in with the client's own IdP (OIDC with PKCE). The portal is authorized like
anything else: every operation is a check against awzy-pdp
([ADR 0020](../../docs/adr/0020-the-portal-is-authorized-by-the-pdp.md)), so the client's
policies decide who can see decisions or change a mode.

Running it locally: [DEVELOPMENT.md](../../DEVELOPMENT.md#signing-in-locally).
