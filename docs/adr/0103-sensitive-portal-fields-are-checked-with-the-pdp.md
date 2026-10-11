# 0103. Sensitive portal fields are checked with the PDP

- Status: Accepted
- Date: 2026-10-11

## Context

The platform plan's day-one guards include field-level authorization through the PDP. The client
portal checked whole areas: whoever may view `decisions` sees every decision, in full. Some fields
are more sensitive than the area around them. A decision's request context can hold IP addresses
and device details, a person's attributes and risk score are personal, and a resource's sharing
and a challenge's justification say more than its name does. A client may want the security team
to see decisions while only its privacy team sees their context.

The options were: GraphQL guards that fail the whole query when a field is denied; a schema
directive per field; or one list of sensitive fields, checked by an extension that leaves out the
field and lets the rest resolve. Failing the query hides the whole page for one field. A directive
spreads the policy across the schema, and the MCP endpoint, which serves the same data, wouldn't
see it.

## Decision

- **A sensitive field is a Cedar entity, `PortalField::"<Type>.<field>"`, a child of its
  `PortalArea`**, and `view` applies to it. The five to start with: `Decision.context`,
  `PrincipalDetail.attributes`, `PrincipalDetail.riskScore`, `ResourceDetail.sharing`, and
  `Challenge.justification`.
- **Base policy permits `view` on every field** (`fields-follow-their-area`). The portal checks a
  field only after the area, so whoever sees an area sees its fields until a client forbids one.
- **One list, in `portals/client/backend/src/fields.rs`**, names the fields and their areas. A
  schema extension checks each listed field before resolving it, asking the PDP once per field per
  request, and resolves a denied one to null. So each listed field is nullable, and the frontend
  shows null as "Hidden by a policy". A PDP that can't answer fails the field with an error rather
  than hiding it.
- **The MCP endpoint hides the same fields** from the tools that return them: a decision's context,
  a principal's attributes and risk, and resources' sharing attributes.

## Consequences

- A client can keep a field to fewer people than its area, with the same policies as everything
  else, and see the decisions they make in the decision log.
- A null on a listed field means "absent" or "hidden"; where both are possible, the portal says so.
- Making another field sensitive means adding it to the list, making it nullable, and naming it in
  the Cedar schema's comment and the docs.
