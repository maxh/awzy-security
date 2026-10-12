# service-auth · service-to-service authentication

How awzy's services inside an install prove who is calling, with Kubernetes service-account
tokens ([ADR 0055](../../../docs/adr/0055-services-authenticate-with-kubernetes-service-account-tokens.md)).
Part of [awzy-authz](../../).

- **Receiving:** `Verifier::from_env("awzy-context")` checks a caller's token against the
  cluster's signing keys, and `require` (axum middleware) lets through only the callers
  (`namespace/service-account`) a route group allows. The chart fills each group, for example
  `AWZY_CONTEXT_WRITERS=awzy-system/awzy`, read with `Allowed::from_env`.
- **Calling:** `TokenSource::from_env().authorization("awzy-context")` reads the projected token
  the chart mounts for that audience, fresh on each call, as the kubelet rotates it.
- On in Kubernetes, off elsewhere (a laptop, a test), since only pods have tokens;
  `AWZY_SERVICE_AUTH=on|off` overrides that.
