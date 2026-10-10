# 0038. Postgres connections use TLS, without verifying the server's certificate yet

- Status: Accepted
- Date: 2026-10-09

## Context

The corp portal and registry crashed on their first deploy: their connection URLs ask for TLS
(`sslmode=require`), and `sqlx` was built without TLS support. Local development never noticed,
because the local Postgres doesn't use TLS. Install services (awzy-policy, awzy-context) connect
to Aurora the same way.

## Decision

- `sqlx` is built with rustls and the webpki roots (`tls-rustls-ring-webpki`).
- Services connect with `sslmode=require`: the connection is encrypted, but the server's
  certificate isn't checked, because RDS and Aurora certificates chain to Amazon's RDS
  certificate authority, which isn't in the webpki roots.
- Locally, URLs without `sslmode` still work against a Postgres without TLS (`prefer` falls back).

## Consequences

- Traffic to Postgres is encrypted, but a host that can intercept traffic inside the VPC could
  impersonate the database. Database security groups only accept awzy's own tasks and nodes,
  which limits that to an attacker already inside the network.
- The next step is `sslmode=verify-full` with the RDS certificate bundle shipped in the images
  (or in the install contract for other clouds' databases). Do that before installs hold real
  client data.
