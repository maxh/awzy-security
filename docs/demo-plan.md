# Awzy Demo Plan

How awzy is shown and tested without customer data: a synthetic company, generated from a
seed, that flows into awzy through the same paths a real client's data does, with the attacker
scenarios from the Beyond Zero paper ([beyond-zero.pdf](beyond-zero.pdf)) built in. The decision
is [ADR 0039](adr/0039-synthetic-organization-for-demo-and-tests.md); where the demo runs is in
the [deployment plan](deployment-plan.md) and [ADR 0027](adr/0027-demo-and-corp-hostnames.md).

## What the demo shows

At `https://demo.awzysecurity.com`, signed in through the demo IdP
([ADR 0032](adr/0032-dev-idp-for-the-demo-install.md)) as a security admin of the synthetic
company:

1. **The directory**: people, teams, contractors, and AI service accounts, provisioned over SCIM.
2. **The world model**: documents and repositories with owners, sharing, and a sensitivity tier
   awzy assigned, with the reasons.
3. **Monitor mode**: a month of normal activity decided per action, with the would-deny and
   would-challenge rates, before anything is enforced.
4. **The scenarios below**, replayed live: each request, its verdict, the challenge it raised, and
   (once the reasoner exists) the containment, with the evidence.
5. **Explain any decision**: which policy and which attributes produced it.

Each step only uses features that exist; the demo grows with the product.

## The synthetic company

A fictional asset-management firm, **Larkspur Capital**, chosen because the paper's scenarios are
about sales, strategy, finance, engineering contractors, and administrators.

| Profile | People | Service accounts | Resources | Work items | Activity |
| --- | --- | --- | --- | --- | --- |
| `demo` | ~200 | 4 | ~5,000 | ~600 | 30 days, ~50k accesses |
| `scale` | 10,000 | 200 | 1,000,000 | 50,000 | 30 days, ~25M accesses |

Same seed, same company: every run of a profile produces identical data, so demos, tests, and
benchmarks are reproducible.

### People and groups

- **Teams**: Sales (Northeast financial services, West Coast technology, EMEA), Strategy, Finance,
  Engineering (Platform, Client Apps), IT administration, Security, Support, Legal, HR.
- **Org chart**: managers, reporting lines, and a team per person, as an IdP would send them.
- **Contractors**: an engineering vendor's staff, in a `contractors` group, with an end date.
- **Administrators**: IT admins with high-risk operations in their normal work.

### Service accounts (AI agents)

- **SalesGenie**: reads sales reports for the rep who prompts it (the paper's rogue agent).
- **CodeHelper**: engineering's coding agent, reads repositories and tickets.
- **SupportBot**: answers customer questions from support articles.
- **FinanceSummarizer**: an unattended agent with an owner and no human present.

Each is registered in awzy-context with an owner ([ADR 0033](adr/0033-service-account-registry-in-awzy-context.md)).

### Resources

- **Documents and folders** in a Drive-like tree per team, with owners, parent folders, sharing
  (team, individuals, company-wide, external), and labels.
- **Repositories** and **tickets** for engineering, linked to the documents they reference.
- **Text**: each document gets a title and a few paragraphs written for its team and tier, so the
  classifier has real input (metadata first, a small model only when metadata can't decide).
- **Sensitivity** (ground truth): about 5% public, 70% internal, 20% confidential, 5% highly
  confidential, the paper's top tier (for example the strategic plan, board materials, M&A notes).
  A share of documents is mislabeled or overshared on purpose, as in real drives.

### Work items

Tickets and accounts assigned to people and agents, linked to the resources they need: a sales
rep's accounts ("Northeast financial services"), an engineer's tickets and repositories, a
contractor's statement of work. This is what "low subject-matter crossover" and "no valid work
assignment" are measured against.

### Normal activity

A month of accesses per person and agent, drawn from their team's resources and their work items,
in working hours, with ordinary noise (shared links, cross-team documents, the odd late night).
It sets peer baselines and gives monitor mode realistic would-deny rates.

## Scenarios

Injected into the activity at fixed times, from the paper unless noted:

| Scenario | What happens | What awzy should do |
| --- | --- | --- |
| **The curious contractor** | Stolen contractor credentials are used to open sensitive documents unrelated to the contractor's statement of work | High-risk accessor, low subject-matter crossover: a document-owner approval challenge |
| **The rogue agent** | SalesGenie, prompted by a Northeast financial-services rep whose risk is rising, queries the highly confidential strategic plan; the rep then tries to export data | No valid work assignment: two challenges (the human confirms the agent's action, then asks the data owner); failing them, plus the slower analysis of recent accesses and exfiltration logs, leads to an automatic containment |
| **The suddenly foolish administrator** | An admin asks basic questions about the system, then runs high-risk operations in rapid, agent-like succession | Risk attribute plus high-risk operation: a security-key challenge |
| **The peer outlier** | One analyst opens 500% more files than the peer group in a week | The reasoner raises risk; a custom challenge or containment |
| **Ordinary work** (not from the paper) | People and agents doing their jobs, including legitimate cross-team access | Allowed, quickly, with few challenges: the false-positive baseline |

The generator records which accesses belong to a scenario, so detection can be measured (below).

## How the data gets in

Through the same paths as a real client's data, so the demo exercises the product, not a
side door:

| Data | Path |
| --- | --- |
| People, groups, contractors | SCIM to awzy-context, as Okta or Entra would send it |
| Service accounts | awzy-context's service-account API (the portal's Directory page uses the same) |
| Resources, work items | A `synthetic` connector behind the `Connector` trait in awzy-connectors, writing through awzy-context's APIs; real connectors (Drive, Jira, GitHub) replace it later |
| Agent activity | A driver that makes real MCP tool calls through awzy-gateway, with tokens from the demo IdP, so every access is a real PDP decision |
| Other activity (app and audit logs) | Events to awzy-intake, then Kafka and ClickHouse, as a log shipper would |

The generator is one Rust crate (`authz/jobs/synthetic`) with a CLI for local runs and a
Kubernetes Job in the chart for the demo install, behind a demo-only value next to `demoIdp`.

## Where it runs

- **Local stack**: a `demo` profile for development and the browser tests.
- **The demo install** (`demo.awzysecurity.com`): seeded on install, then the driver replays a
  day of activity on a schedule so the demo always has fresh decisions.
- **Scale tests**: the `scale` profile against the example install, per the deployment plan's
  performance scenarios.

## Scale and performance

The `scale` profile is also the load for the performance tests in the
[deployment plan](deployment-plan.md#performance-testing), which has where they run (the example
install), how they're watched (Managed Prometheus and Grafana, X-Ray), and the load generator
(k6 in the install's VPC, with tokens from the demo IdP's client-credentials and token-exchange
flows, so every request is a real check).

**Target load (proposed).** Sized for the largest design partner likely early: 10,000 employees
and 500 agents, which is the `scale` profile. At peak about 30% of people are active with a
checked request every ~10 s (≈300/s) and about 20% of agents make ~2 tool calls/s (≈200/s):
≈500 checks/s, times 3 for headroom. Revise with a design partner's real numbers.

| What | Target (proposed) |
| --- | --- |
| Sustained load | 1,500 checks/s for 30 minutes, PDP p99 under 10 ms, client-side |
| Burst | 5,000 checks/s for 1 minute, p99 under 25 ms, no errors |
| Breaking point | Raise load until p99 passes 50 ms or errors appear; record where |
| Resource lookups | Cache misses p99 under 3 ms (Valkey); hit rate over 90% with skewed access (a few hot documents, a long tail) |
| PDP memory | 10,000 people and 500 agents: under 512 MB per pod, startup under 30 s; 100,000 people measured, not pass/fail |
| Ingest | 1,500 decisions/s and 3,000 events/s into ClickHouse, consumer lag under 60 s |
| Connector load | 1,000,000 resources into awzy-context in under 2 hours |
| Freshness | Containment under 2 s; attribute and resource changes under 60 s (the platform plan's targets) |
| Classification | Cost per 1,000 resources, measured and written into the platform plan |
| Query load | 25,000,000 accesses in ClickHouse: decision search and explain under 1 s |

Each run is reported in `docs/perf/` with the profile, the seed, the commit, and the numbers.

## Measuring with ground truth

Because the generator knows the truth, the demo doubles as an evaluation:

- **Classification**: tier assigned by awzy versus the generated tier, per tier, and the LLM
  budget it took.
- **Detection**: for each scenario, whether and when awzy challenged or contained, and how many
  ordinary accesses it challenged (false positives).
- **Latency**: PDP p99 under the scale profile.

These run in CI on the `demo` profile once the pieces exist, so a change that breaks a scenario
fails a PR.

## What synthetic data won't tell us

- Real sources' quirks: permission models, pagination, rate limits, webhooks, and deleted or
  moved items. The first real connector tests those.
- How messy real documents are. Classification looks better on generated text than it will on a
  client's drive.
- What a design partner's people actually do. Monitor mode at a design partner is still the real
  test of false-positive rates.

## Build order

Each step is usable on its own and shows up in the demo:

1. **Generator and directory**: the crate, seeded people, groups, contractors, and service
   accounts, sent over SCIM and the service-account API. The Directory page shows Larkspur.
2. **Resources in awzy-context**: the model, storage, and API, and the `synthetic` connector
   writing resources and work items.
3. **Resources at decision time**: the PDP loads resource entities by ID (Valkey with a local
   cache); the gateway sends IDs, not attributes.
4. **Classification and work assignments**: metadata-first sensitivity, and a work-assignment
   attribute policies can use.
5. **Activity**: the MCP driver and intake events; monitor mode fills with real decisions.
6. **Scenarios with challenges**: the curious contractor and the first half of the rogue agent,
   once awzy-challenge exists.
7. **Scenarios with reasoning**: the peer outlier, the foolish administrator, and the rogue
   agent's containment, once awzy-reasoner exists. The foolish administrator runs end to end: awzy-reasoner
   scores his basic questions as elevated risk, and his high-risk operations on `larkspur-admin`
   ask for his security key ([ADR 0062](adr/0062-unfamiliar-territory-raises-risk-and-high-risk-operations-need-a-key.md)).
