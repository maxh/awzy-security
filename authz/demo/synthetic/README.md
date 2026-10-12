# awzy-synthetic · the demo's synthetic company

Generates Larkspur Capital, the synthetic company in the [demo plan](../../../docs/plans/demo-plan.md)
([ADR 0039](../../../docs/adr/0039-synthetic-organization-for-demo-and-tests.md)), and sends it to
[awzy-context](../../services/context/) the way a client's IdP would. Part of
[awzy-authz](../../).

- **Directory** (demo plan step 1): people with Okta/Entra-style SCIM resources, team and
  department groups, contractors, managers, and AI service accounts (SalesGenie, CodeHelper,
  SupportBot, FinanceSummarizer). `demo` ≈ 200 people, `scale` ≈ 10,000.
- **World** (demo plan step 2): each team's folders and documents, engineering repositories, the
  scenarios' documents at fixed IDs (`synthetic:doc:strategic-plan-fy27`, …), and work items
  (sales accounts, with SalesGenie on Nora Patel's; engineering tickets; Quarry Labs' statement
  of work), sent through awzy-context's batch API. Only labeled documents carry a sensitivity;
  the rest arrive unclassified. The true tier stays in the generator, to score classification
  against. `demo` ≈ 4,000 documents, `scale` ≈ 1,000,000, generated a person at a time.
- **larkspur-drive** (`awzy-synthetic drive`): an MCP server with `list_folder` and
  `read_document` over the documents in awzy-context, to put behind awzy-gateway so agents'
  tool calls are decided on real documents
  ([ADR 0050](../../../docs/adr/0050-mcp-tool-calls-checked-on-the-resource.md)), and **larkspur-admin**
  at `/admin/mcp` in the same process: IT's high-risk operations (`reset_password`,
  `disable_mfa`, `rotate_admin_credentials`, `export_directory`), which change nothing
  ([ADR 0064](../../../docs/adr/0064-unfamiliar-territory-raises-risk-and-high-risk-operations-need-a-key.md)).
- **Activity** (demo plan step 5b, `awzy-synthetic activity`): a seeded day of who opens what,
  mostly their work's and their team's documents, with agents acting for their owners and a
  little noise, made as tool calls on larkspur-drive through awzy-gateway, so each is a real
  decision. Each access records why it was generated, to score detection against later.
- **Scenarios** (demo plan step 6): the curious contractor (Sam's stolen credentials at 2am on the
  payments design, which is shared with all of engineering but not his statement of work, and on
  the client holdings export, which isn't shared with him) and the rogue agent's first half
  (SalesGenie, for Nora, on the overshared FY27 strategic plan); and (step 7) the suddenly foolish
  administrator (Lee reads engineering's introductions for new starters, then runs high-risk
  operations within a minute), the peer outlier (Riley opens sixty of finance's documents in a
  morning, about five times his day and his team's), and the rogue agent's ending (the plan's
  owner rejects SalesGenie's request, Nora exports her accounts, and awzy-reasoner contains her,
  [ADR 0066](../../../docs/adr/0066-the-reasoner-contains-people-automatically-in-its-own-mode.md)). `activity` adds them to its first day and reports how many awzy
  caught and how much ordinary work it challenged. A test runs them end to end through
  awzy-context, the PDP, larkspur-drive, larkspur-admin, and awzy-gateway, with awzy-reasoner's
  scores ([ADR 0061](../../../docs/adr/0061-ownership-and-sharing-in-the-resource-entity.md),
  [ADR 0064](../../../docs/adr/0064-unfamiliar-territory-raises-risk-and-high-risk-operations-need-a-key.md)).
- **History** (demo plan step 5c, `awzy-synthetic history --days 30`): the same seeded days,
  placed on the workdays before today and sent to awzy-intake as app events from
  `larkspur-drive`, so baselines have a month to learn from. Why each access was generated
  isn't sent; it's regenerated from the seed to score detection.
- The same seed always gives the same company; running it again replaces rather than duplicates.
- Demo installs run it as a Job after each sync (`demoData.enabled`); never on a real install.
  With `demoData.activity.enabled` they also run larkspur-drive behind awzy-gateway and `activity`
  each weekday, signing its tokens as the demo IdP.

 Running it locally:
[DEVELOPMENT.md](../../../DEVELOPMENT.md#the-synthetic-company-adr-0039).
