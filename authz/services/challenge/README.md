# awzy-challenge · exceptions

Challenge and containment state machines. Part of [awzy-authz](../../README.md).

Supports justification (checked against the resource by an LLM), WebAuthn security key, approval
by manager or data owner, and biometric checks through an external provider. Sends prompts to
Slack, Teams, or email.

- Durable state in Postgres
- A passed challenge becomes a short-lived attribute the PDP reads
- The LLM justification check is one signal. It can't pass a challenge alone on
  high-sensitivity resources, which also need a security key or an approval

A skeleton so far ([platform plan](../../../docs/platform-plan.md#built-so-far)).
