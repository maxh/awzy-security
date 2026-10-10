// The static floor. Clients add their own bundles through awzy-policy.

@id("security-team-read")
permit (principal in Group::"security", action == Action::"read", resource);

@id("engineering-read-internal")
permit (principal in Group::"engineering", action == Action::"read", resource)
when { resource.sensitivity == "public" || resource.sensitivity == "internal" };

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

// Approval challenges (awzy-challenge): the security team approves. Clients add data owners and
// managers as connectors bring those relationships in.
@id("security-team-approves")
permit (principal in Group::"security", action == Action::"approve", resource);

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
