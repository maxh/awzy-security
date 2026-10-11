// The static floor. Clients add their own bundles through awzy-policy.

@id("security-team-read")
permit (principal in Group::"security", action == Action::"read", resource);

@id("engineering-read-internal")
permit (principal in Group::"engineering", action == Action::"read", resource)
when { resource.sensitivity == "public" || resource.sensitivity == "internal" };

// Resources from connectors (ADR 0061): their owner and whoever they're shared with, including
// through their folders, read them; owners and writers change them.
@id("owners-and-shares-read")
permit (principal is Human, action == Action::"read", resource is Resource)
when {
  (resource has owner && resource.owner == principal) ||
  (resource has shared_humans && resource.shared_humans.contains(principal)) ||
  (resource has shared_groups && principal in resource.shared_groups) ||
  (resource has shared_with_everyone && resource.shared_with_everyone)
};

@id("owners-and-writers-write")
permit (principal is Human, action == Action::"write", resource is Resource)
when {
  (resource has owner && resource.owner == principal) ||
  (resource has writer_humans && resource.writer_humans.contains(principal)) ||
  (resource has writer_groups && principal in resource.writer_groups)
};

// A service account reads what's shared with its groups or with everyone. Acting for someone, the
// PDP checks that person too and the stricter verdict wins, so an agent never gets more than
// both of them could (ADR 0016).
@id("service-accounts-read-group-shares")
permit (principal is ServiceAccount, action == Action::"read", resource is Resource)
when {
  (resource has shared_groups && principal in resource.shared_groups) ||
  (resource has shared_with_everyone && resource.shared_with_everyone)
};

// MCP tools through awzy-gateway: low-sensitivity tools for everyone, any tool for security.
@id("call-low-sensitivity-tools")
permit (principal, action == Action::"call", resource is McpTool)
when { resource.sensitivity == "public" || resource.sensitivity == "internal" };

@id("security-team-calls-any-tool")
permit (principal in Group::"security", action == Action::"call", resource is McpTool);

// IT administrators' work includes high-risk operations: resetting credentials, changing MFA,
// exporting and restoring the directory.
@id("it-calls-any-tool")
permit (principal in Group::"it", action == Action::"call", resource is McpTool);

// The client portal (ADR 0020): the security team runs it.
@id("security-team-runs-the-portal")
permit (
  principal in Group::"security",
  action in [Action::"view", Action::"administer"],
  resource is PortalArea
);

// Sensitive fields (ADR 0103): whoever sees an area sees its fields, which the portal checks
// only after the area. A client hides one with a forbid, e.g. decisions' request context for all
// but its privacy team:
//   forbid (principal, action == Action::"view", resource == PortalField::"Decision.context")
//   unless { principal in Group::"privacy" };
@id("fields-follow-their-area")
permit (principal, action == Action::"view", resource is PortalField);

// Switching between monitor and enforce changes what awzy blocks for everyone. The security team's
// people switch it, each time with a touch of their security key, so neither a stolen session nor
// an agent working with someone's credentials can.
@id("security-team-switches-enforcement")
permit (
  principal is Human in Group::"security",
  action == Action::"switch_enforcement",
  resource == PortalArea::"enforcement"
);

@id("enforcement-switches-need-a-key")
@challenge("verification")
forbid (principal, action == Action::"switch_enforcement", resource);

// Support sessions (ADR 0095): an awzy staff member views the portal areas a client admin approved,
// and nothing else, until the session ends. The client portal names those areas, from the approved
// session, only on that staff member's checks, so these never touch anyone else.
@id("support-views-approved-areas")
permit (principal, action == Action::"view", resource is PortalArea)
when { context has support_areas && context.support_areas.contains(resource) };

@id("support-views-no-other-area")
forbid (principal, action == Action::"view", resource is PortalArea)
when { context has support_areas && !context.support_areas.contains(resource) };

@id("support-only-views")
forbid (principal, action, resource)
when { context has support_areas }
unless { action == Action::"view" };

// Everyone answers the challenges sent to them in the client portal. Seeing every challenge and
// containing principals is `administer`, the security team's above.
@id("humans-answer-their-challenges")
permit (principal is Human, action == Action::"view", resource == PortalArea::"challenges");

// Approval challenges (awzy-challenge): the security team's people approve. Clients add data owners
// and managers as connectors bring those relationships in, and agents by name (ADR 0065).
@id("security-team-approves")
permit (principal is Human in Group::"security", action == Action::"approve", resource);

// Data owners approve access to what they own (ADR 0061).
@id("owners-approve")
permit (principal is Human, action == Action::"approve", resource is Resource)
when { resource has owner && resource.owner == principal };

// An agent never approves for the person it answers to, or the approval is that person's own
// (ADR 0065). Without `approving_for` there's no telling, so it doesn't approve at all.
@id("agents-never-approve-for-their-owner")
forbid (principal is ServiceAccount, action == Action::"approve", resource)
unless { context has approving_for && principal.owner != context.approving_for };

// Crown jewels need a person's approval, whatever a client's policies let agents approve.
@id("crown-jewels-need-a-human-to-approve")
forbid (principal is ServiceAccount, action == Action::"approve", resource)
when { resource.sensitivity == "crown_jewels" };

// Deprovisioned in the IdP: no access, whatever their token says (ADR 0031).
@id("deactivated-humans")
forbid (principal is Human, action, resource)
when { principal has active && !principal.active };

// Dynamic ceiling: high risk turns an allowed action into a security-key challenge.
@id("high-risk-verify")
@challenge("verification")
forbid (principal, action, resource)
when { context.risk_score > 80 };

// The suddenly foolish administrator (ADR 0064): someone whose risk is elevated runs a high-risk
// operation only after touching their security key, which an agent working with their credentials
// can't do for them. Checked for a person, so an agent acting for one is stopped too.
@id("elevated-risk-high-risk-operations-verify")
@challenge("verification")
forbid (principal is Human, action == Action::"call", resource is McpTool)
when { resource.sensitivity == "crown_jewels" && context.risk_score > 40 };

// AI acting on its own authority never touches crown jewels.
@id("unattended-ai-no-crown-jewels")
forbid (principal is ServiceAccount, action, resource)
when { principal.ai && !(context has on_behalf_of) && resource.sensitivity == "crown_jewels" };

// Software working with a person's own credentials, such as a coding agent signed in as them, is
// that person to awzy (ADR 0085). When awzy-reasoner scores someone's requests as an agent's, their
// reads, writes, and tool calls on sensitive data need their security key, which no agent can
// touch.
@id("automated-people-verify")
@challenge("verification")
forbid (
  principal is Human,
  action in [Action::"read", Action::"write", Action::"call"],
  resource
)
when {
  context has automation_score && context.automation_score > 80 &&
  (resource.sensitivity == "confidential" || resource.sensitivity == "crown_jewels")
};

// A service account acting for someone who isn't there needs approval when risk is elevated.
@id("absent-human-approval")
@challenge("approval")
forbid (principal is ServiceAccount, action, resource)
when { context has on_behalf_of && !context.human_present && context.risk_score > 50 };

// The curious contractor: a contractor reaching sensitive data that none of their work links (their
// statement of work, ADR 0046) needs its owner's approval, however widely it's shared.
@id("contractors-outside-their-work")
@challenge("approval")
forbid (principal in Group::"contractors", action in [Action::"read", Action::"write"], resource is Resource)
when {
  (resource.sensitivity == "confidential" || resource.sensitivity == "crown_jewels") &&
  context has assigned && !context.assigned
};

// The rogue agent: an AI agent reaching sensitive data for someone, outside any work that links it,
// needs that person to say why and the data's owner to approve.
@id("agents-outside-their-work-justify")
@challenge("justification")
forbid (principal is ServiceAccount, action in [Action::"read", Action::"write"], resource is Resource)
when {
  principal.ai && context has on_behalf_of &&
  (resource.sensitivity == "confidential" || resource.sensitivity == "crown_jewels") &&
  context has assigned && !context.assigned
};

@id("agents-outside-their-work-approval")
@challenge("approval")
forbid (principal is ServiceAccount, action in [Action::"read", Action::"write"], resource is Resource)
when {
  principal.ai && context has on_behalf_of &&
  (resource.sensitivity == "confidential" || resource.sensitivity == "crown_jewels") &&
  context has assigned && !context.assigned
};
