# 0029. HTTPS in local development too; no plain-HTTP exception for signing keys

- Status: Accepted
- Date: 2026-10-09
- Amends: [0024](0024-local-frontend-against-a-deployed-backend.md) (local frontends serve `https://localhost`)

## Context

awzy validates tokens with keys fetched from the IdP's JWKS endpoint. Keys fetched over plain HTTP
could be swapped in transit and used to forge tokens, so real installs must use HTTPS. The local
dev IdP served plain HTTP, which forced an exception for loopback addresses into that code path.
An exception in security-sensitive code is a way to misconfigure a real install, and it meant
local development never exercised the TLS path.

Let's Encrypt can't help locally: it only issues certificates for public names it can verify,
and nobody owns `localhost`. Pointing a name under `awzysecurity.com` at `127.0.0.1` would work
but needs a Cloudflare token per machine, a publicly trusted private key on every laptop, renewal
every 90 days, and fails on networks that block public names resolving to loopback.

## Decision

- **Signing keys are fetched over HTTPS only,** with no exception for loopback. An IdP behind a
  private CA is supported by trusting that CA (`jwks_ca_file`, `AWZY_AUTH_CA_FILE`).
- **The dev IdP serves HTTPS.** Its certificate comes from, in order: explicit files; mkcert
  files in `.dev-certs/` (gitignored); or a self-signed certificate generated at startup, whose
  PEM it writes out for the portal to trust.
- **Developers use mkcert** for `localhost`, `127.0.0.1`, and `::1`. `mkcert -install` makes the
  browser trust it once; the portal trusts it through `AWZY_AUTH_CA_FILE`, the same setting a
  real install with a private CA uses, so local development exercises the production code.
- **Tests and CI use the self-signed certificate.** The browser test tells headless Chrome to
  accept it; nothing needs installing.
- **The Vite dev servers serve `https://localhost`** when the mkcert files are present, plain HTTP
  otherwise. Running a local frontend against a deployed backend (0024) then uses
  `https://localhost:5173/auth/callback` (`:5174` for corp), and the corp session cookie, which is
  `Secure`, works in every browser.
- **Deployed environments** (`corp.awzysecurity.com`, `demo.awzysecurity.com`, 0027) get publicly
  trusted certificates where their load balancer runs, as 0027 says. That is where Let's Encrypt
  fits, not on laptops.

## Consequences

- One-time setup per developer: `brew install mkcert && mkcert -install`, then generate the
  `.dev-certs/` files (README).
- Local backends still listen on plain HTTP on loopback. The browser reaches them only through
  the Vite dev server, and in a cluster TLS ends at the ingress.
- `awzy-reporter` still sends heartbeats to a local corp portal over plain HTTP. Requiring HTTPS
  there needs TLS on the local corp backend; until then a deployed reporter must be given an
  `https://` corp URL.
