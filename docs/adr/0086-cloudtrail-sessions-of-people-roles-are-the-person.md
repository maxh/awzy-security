# 0086. CloudTrail sessions of people roles are recorded as the person

- Status: Accepted
- Date: 2026-10-11

## Context

awzy-intake kept every CloudTrail caller as CloudTrail named it, an `AwsPrincipal` ARN, so a
person's activity in AWS counted toward nobody's risk score. Most people reach AWS through IAM
Identity Center, whose sessions are `arn:aws:sts::<account>:assumed-role/AWSReservedSSO_<set>_<id>/<user name>`:
the session name is their user name in the IdP, the same one SCIM provisions to awzy-context.

## Decision

- **awzy-intake maps those sessions to the person** at ingest: the event's principal becomes
  `Human::<session name>`, and the ARN is kept in its attributes as `awsPrincipal`.
- **Identity Center's roles always count.** An install can name other roles whose session names
  are people (`intake.humanRoles`), such as a SAML-federated role that sets the session name to the
  user's email.
- **Everything else stays as CloudTrail named it.** Mapping a workload's role to a registered
  service account would need a registry of role ARNs, and waits for one.

## Consequences

- A person's AWS activity counts toward their risk and automation scores like any other.
- A session name that isn't the person's awzy user name (a role that sets something else) maps
  to nobody useful; such roles shouldn't be listed.
