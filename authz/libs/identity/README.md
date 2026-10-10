# identity

Library: who is calling, validated from a bearer token, never from headers or tool arguments
([ADR 0008](../../../docs/adr/0008-agent-identity-through-token-exchange.md)). Shared by
awzy-gateway and the client portal. Part of [awzy-authz](../../README.md).

- No `act` claim, `sub` not a registered service account: a person (`Human`), with groups
  from the `groups` claim.
- No `act` claim, `sub` a registered service account: it acts on its own authority
  (client-credentials token).
- An `act` claim (RFC 8693 token exchange): `act.sub` is a service account acting for the
  person in `sub` ([ADR 0016](../../../docs/adr/0016-human-and-serviceaccount-principals.md)).

Keys come from the IdP's JWKS (ES256 or RS256), fetched over HTTPS only, with an optional
private CA. See [Local HTTPS](../../../DEVELOPMENT.md#local-https).
