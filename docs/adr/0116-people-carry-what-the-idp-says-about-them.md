# 0116. People carry what the IdP says about them

- Status: Accepted
- Date: 2026-10-12
- Builds on: [0031](0031-people-and-groups-from-the-idp-over-scim.md) (people and groups over SCIM)

## Context

The Beyond Zero paper's world model knows each person's "job function, role, seniority" from HR
and identity systems. Under 0031 awzy-context keeps the whole SCIM resource the IdP sends, but the
`Human` entity the PDPs see has only `active` and its groups. So a policy can say "contractors"
only through a group someone keeps by hand, although Okta and Entra ID already send a title,
department, and user type with every person (#355).

## Decision

- **The `Human` entity carries six optional attributes, from SCIM:** `title` (core `title`),
  `employee_type` (core `userType`, else an enterprise-extension `employeeType`), and from the
  enterprise extension `department`, `division`, `organization`, and `cost_center`. Each is absent
  when the IdP doesn't send it, so policies test it with `has`. Nothing else in the resource
  becomes an attribute; awzy-context still keeps all of it.
- **`employee_type` is a slug:** lowercase, with `_` for spaces and hyphens, so "Contractor" and
  "Full-Time" are `contractor` and `full_time`. IdPs spell the same types differently, and
  policies compare it to a fixed few values. The others are kept as the IdP sends them, trimmed:
  they're the client's own names for departments and titles, which policies quote.
- **Derived when the snapshot is built**, from the stored resource, so people provisioned before
  this have them without a migration or a re-push.
- **A bundle published before these existed still loads:** the PDP drops attributes a bundle's
  schema doesn't declare, as it does for `Resource` attributes added after the first schema.
- **The base policy's contractor** is anyone in the `contractors` group or with `employee_type`
  `contractor`, so a client whose IdP sends user types needs no group for it.
- **Manager and team are not in this:** a manager is a reference to another person, which needs
  resolving SCIM IDs to people, and a team is a choice of which groups count. Both are #356.

Rejected: a client-configured mapping from any SCIM path to any attribute. It's flexible, but
every install would then have its own schema, and the base policies couldn't rely on any of it.
A fixed set from the standard schema covers what Okta and Entra ID send by default; a custom
attribute can come later as an app-owned type (0108) or a mapping ADR when a client needs one.

## Consequences

- Policies can name a person's department, title, and employee type, kept current by the IdP.
- What a person's IdP record says is now trusted as much as their groups. Whoever can edit a
  title or user type in the IdP can change decisions; that was already true of groups.
- A contract's end date has no standard SCIM attribute, so it isn't one here; an IdP that
  deactivates a person at the end of their contract already denies them through `active`.
