# awzy-challenge · exceptions

Challenges and containments. Part of [awzy-authz](../../README.md).
Design: [ADR 0054](../../../docs/adr/0054-challenges-and-containments-over-grpc.md).

- **Challenges.** On an enforced challenge verdict, the PDP calls `Resolve`. A challenge opens
  for that exact request and goes to the person responsible: the human principal, the person an
  agent acts for, or a service account's owner. Retries share it. Once every kind is passed, the
  request is allowed on retry for `AWZY_CHALLENGE_GRANT_SECS`.
- **Justification:** the answerer writes why they need the access.
- **Approval:** someone else whom the PDP allows to `approve` the resource. A rejection denies
  the request until it expires.
- **Verification:** a touch of a security key the answerer enrolled (WebAuthn, ES256 only;
  [ADR 0059](../../../docs/adr/0059-security-keys-for-verification-challenges.md)). A first key
  enrolls on the person's sign-in; adding or removing one needs a key they already have.
- **Containments** revoke a principal's access until a person lifts them. Every PDP follows
  `WatchContainments` (the whole list on change, with a heartbeat every 500 ms) and denies
  contained principals without evaluating policy.
- Durable state in Postgres (`AWZY_POSTGRES_URL`). Not ready until the containment list has
  loaded.

| Setting | Default | |
| --- | --- | --- |
| `AWZY_CHALLENGE_GRANT_SECS` | 900 | How long a passed challenge allows its request |
| `AWZY_CHALLENGE_PENDING_SECS` | 86400 | How long a challenge waits, and a rejection blocks |
| `AWZY_CHALLENGE_WEBAUTHN_ORIGINS` | unset | The portal's origins, comma-separated, e.g. `https://portal.example.com` |
| `AWZY_CHALLENGE_WEBAUTHN_RP_ID` | unset | The portal's hostname; unset turns security keys off |
| `AWZY_PDP_ADDR` | unset | The PDP that decides who may approve; unset refuses approvals |
| `AWZY_POSTGRES_URL` | required | Its own database |

Not yet:
- Biometric checks
- The LLM check of a justification against the resource; it can't pass a challenge alone on
  high-sensitivity resources, which also need a security key or an approval
- Prompts over Slack, Teams, or email
- Lifting a containment by challenge
- Valkey for grants and the feed

The client portal's Challenges page is where people answer and admins approve and contain
(`portals/client/backend/src/challenges.rs`). The portal sends the approver's entities from their
token with `Decide`, so the PDP's `approve` check sees their groups even without awzy-context.

Integration tests: `bazel test --config=local-stack //authz/services/challenge:challenge_test`.

Callers need a service token (ADR 0055) from the group a method belongs to: the PDPs resolve
challenges and follow containments; only the client portal reads and answers challenges for a
person, since `answered_by` and `decided_by` are taken as given; the portal and the backend
contain and lift; MCP gateways follow containments for their outage fallback. The chart sets the
groups (`AWZY_CHALLENGE_ANSWERERS`, `_CONTAINERS`, `_GATEWAYS`, `_PDPS`). In the chart the
gateway runs as the backend's service account, which `_PDPS` already allows.
