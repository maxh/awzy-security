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
  (resource has shared_people && resource.shared_people.contains(principal)) ||
  (resource has shared_groups && principal in resource.shared_groups) ||
  (resource has shared_with_everyone && resource.shared_with_everyone)
};

@id("owners-and-writers-write")
permit (principal is Human, action == Action::"write", resource is Resource)
when {
  (resource has owner && resource.owner == principal) ||
  (resource has writer_people && resource.writer_people.contains(principal)) ||
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

// The client portal (ADR 0020): the security team runs it.
@id("security-team-runs-the-portal")
permit (
  principal in Group::"security",
  action in [Action::"view", Action::"administer"],
  resource is PortalArea
);

// Everyone answers the challenges sent to them in the client portal. Seeing every challenge and
// containing accessors is `administer`, the security team's above.
@id("people-answer-their-challenges")
permit (principal is Human, action == Action::"view", resource == PortalArea::"challenges");

// Approval challenges (awzy-challenge): the security team approves. Clients add data owners and
// managers as connectors bring those relationships in.
@id("security-team-approves")
permit (principal in Group::"security", action == Action::"approve", resource);

// Data owners approve access to what they own (ADR 0061).
@id("owners-approve")
permit (principal is Human, action == Action::"approve", resource is Resource)
when { resource has owner && resource.owner == principal };

// Deprovisioned in the IdP: no access, whatever their token says (ADR 0031).
@id("deactivated-people")
forbid (principal is Human, action, resource)
when { principal has active && !principal.active };

// Dynamic ceiling: high risk turns an allowed action into a security-key challenge.
@id("high-risk-verify")
@challenge("verification")
forbid (principal, action, resource)
when { context.risk_score > 80 };

// AI acting on its own authority never touches crown jewels.
@id("unattended-ai-no-crown-jewels")
forbid (principal is ServiceAccount, action, resource)
when { principal.ai && !(context has on_behalf_of) && resource.sensitivity == "crown_jewels" };

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
