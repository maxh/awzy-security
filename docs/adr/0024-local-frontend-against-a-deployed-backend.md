# 0024. Local portal frontends can run against a deployed backend

- Status: Accepted, amended by [0025](0025-corp-portal-passwords-for-demos-and-testing.md) (corp sign-in) and [0029](0029-https-in-local-development.md) (`https://localhost`)
- Date: 2026-10-09

## Context

Frontend work on either portal needs realistic data: real decision volumes, real policy bundles,
real identities. Getting that locally means running the whole stack and filling it with data. A
deployed install already has it, but the frontend there only changes on release.

Each portal backend serves its built frontend (0012), so in a deployment the browser talks to a
single origin, and the backends have no CORS configuration.

## Decision

- Each portal frontend's Vite dev server proxies its API paths to a backend chosen with
  `AWZY_BACKEND`: `/graphql` for both portals, and `/auth/config` for the client portal. Without
  it, the proxy goes to the local backend (client portal :8080, corp portal :8090).
- The proxy rewrites the Host header to the deployed backend's own. The browser still sees one
  origin, `localhost`, so the deployed backend needs no CORS and no change at all to support this.
- Sign-in is real. The developer signs in with the install's IdP, and the backend authorizes them
  through its PDP like anyone else (0020). There is no bypass and no developer token.
- Redirects to `localhost` need a separate IdP client, named with `VITE_AWZY_CLIENT_ID`, that allows
  `http://localhost:5173/auth/callback` (client portal) or `:5174` (corp portal).

## Where it can point

- **Client portal:** only at awzy's own staging and demo installs. A client's install must not
  register a localhost redirect URI. Its data is the client's, and awzy staff have no access to it
  (0009).
- **Corp portal:** at awzy's own deployments, staging and production. It holds no client data, and
  the people using this mode are the staff who are its users anyway.

## Consequences

- A frontend change can be checked against real data before release without deploying anything.
- The dev IdP client is a credential surface: anyone who can run the frontend locally and sign in
  can use it. It grants nothing on its own, because the backend still decides every operation, but
  each install's IdP should limit it to staff accounts.
- The corp portal has no sign-in yet. Until it does, a deployed corp backend is open to anyone who
  can reach it, with or without this mode. Corp sign-in, against awzy's own IdP, has to land before
  the corp portal is deployed anywhere reachable.
