# service-auth · service-to-service authentication

How awzy's services inside an install prove who is calling, with Kubernetes service-account
tokens ([ADR 0054](../../../docs/adr/0054-services-authenticate-with-kubernetes-service-account-tokens.md)).
Part of [awzy-authz](../../README.md).

- **Receiving:** `Verifier::from_env("awzy-context")` checks a caller's token against the
  cluster's signing keys, and `require` (axum middleware) lets through only the service accounts
  a route allows, in the install's own namespaces.
- **Calling:** `TokenSource::from_env().authorization("awzy-context")` reads the projected token
  the chart mounts for that audience, fresh on each call, as the kubelet rotates it.
- `AWZY_SERVICE_AUTH=off` turns both off for local development and tests.
