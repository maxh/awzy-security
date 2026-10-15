// What base.cedar promises, proved by //authz/libs/policy-proofs over every request the schema
// allows (#428). Each property is one permit allowing everything the property allows; base.cedar
// keeps it when it allows nothing more.
//
// `@holds("outright")`: nothing outside the property is allowed without a challenge.
// `@holds("after_challenges")`: nothing outside it is allowed even once every challenge is
// passed, so no challenge, however answered, gets there.
//
// Each property must also rule something out: one that "permit everything" keeps fails.

@id("deactivated-humans-are-never-allowed")
@holds("after_challenges")
permit (principal, action, resource)
unless { principal is Human && principal has active && !principal.active };

@id("unattended-ai-never-touches-crown-jewels")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  principal is ServiceAccount && principal.ai && !(context has on_behalf_of) &&
  resource has sensitivity && resource.sensitivity == "crown_jewels"
};

@id("only-the-security-team-administers-and-audits")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  (action == Action::"administer" || action == Action::"audit") && !(principal in Group::"security")
};

@id("agents-never-switch-enforcement-or-configure-connectors")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  principal is ServiceAccount &&
  (action == Action::"switch_enforcement" || action == Action::"configure_connectors")
};

@id("enforcement-switches-need-a-key")
@holds("outright")
permit (principal, action, resource)
unless { action == Action::"switch_enforcement" };

@id("connector-configuration-needs-a-key")
@holds("outright")
permit (principal, action, resource)
unless { action == Action::"configure_connectors" };

@id("support-sessions-only-view")
@holds("after_challenges")
permit (principal, action, resource)
unless { context has support_areas && action != Action::"view" };

@id("support-sessions-see-only-approved-areas")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  context has support_areas && resource is PortalArea && !context.support_areas.contains(resource)
};

@id("agents-never-approve-for-their-owner")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  principal is ServiceAccount && action == Action::"approve" &&
  !(context has approving_for && principal.owner != context.approving_for)
};

@id("crown-jewels-need-a-human-to-approve")
@holds("after_challenges")
permit (principal, action, resource)
unless {
  principal is ServiceAccount && action == Action::"approve" &&
  resource has sensitivity && resource.sensitivity == "crown_jewels"
};

@id("high-risk-is-never-allowed-outright")
@holds("outright")
permit (principal, action, resource)
unless { context.risk_score > 80 };

@id("elevated-risk-crown-jewel-tools-need-a-key")
@holds("outright")
permit (principal, action, resource)
unless {
  principal is Human && action == Action::"call" && resource is McpTool &&
  resource.sensitivity == "crown_jewels" && context.risk_score > 40
};
