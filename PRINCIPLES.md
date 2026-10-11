# Principles

The rules awzy's design follows, each with the decision records that settled it. When a change
would break one, it needs a new ADR, not an exception in code. The plans are in
[docs/plans/](docs/plans/), and the decisions in [docs/adr/](docs/adr/README.md).

## How awzy decides

- **Precompute, then decide fast.** Anything that needs a model or a large scan runs ahead of
  time and writes attributes. A check only reads attributes and evaluates policy: the PDP never
  calls a model or queries ClickHouse, so a decision takes milliseconds
  ([platform plan](docs/plans/platform-plan.md#summary)).
- **One PDP for every access path.** Enforcement points differ in how they see a request, not in
  how it's decided ([ADR 0003](docs/adr/0003-one-pdp-for-every-access-path.md)).
- **Delegation never adds access.** An agent acting for a person is checked as itself and as the
  person, and the stricter verdict wins
  ([ADR 0016](docs/adr/0016-human-and-serviceaccount-principals.md)).
- **Monitor first.** Every enforcement scope starts by recording what it would do; a client
  switches it to enforce after reviewing that
  ([ADR 0066](docs/adr/0066-the-reasoner-contains-people-automatically-in-its-own-mode.md),
  [ADR 0080](docs/adr/0080-signed-enforcement-modes-and-grants-per-policy.md)).
- **Fail safe.** A PDP that hasn't loaded policy and mode refuses checks. An enforcement point that
  ignores the mode applies the verdict. Sensitive tools fail closed when the PDP is unreachable,
  and an outage never replays an allow for a contained principal
  ([ADR 0030](docs/adr/0030-gateway-outage-fallback.md)). Unknown resources are marked
  `unclassified`, not trusted ([ADR 0044](docs/adr/0044-pdp-loads-resources-from-awzy-context.md)).
- **Deterministic first, model second.** Rules and scores set the floor. A model can raise risk or
  a tier but never lower one, and its findings are checked in code before anything uses them
  ([ADR 0047](docs/adr/0047-metadata-first-classification-rules.md),
  [ADR 0056](docs/adr/0056-awzy-scores-principals-risk-and-the-pdp-uses-it.md),
  [ADR 0067](docs/adr/0067-investigations-are-checked-findings-measured-against-a-compromised-model.md)).
- **Agents propose, people decide.** Agents can draft a policy or ask for a lift; a person
  publishes or lifts. Agents can approve where a client's policy names them, but only a person
  verifies with a security key ([ADR 0065](docs/adr/0065-agents-approve-people-verify.md)).

## Where data goes

- **Client data never leaves the install.** awzy has no access to an install, even in an
  emergency. The only thing sent to awzy is the heartbeat, in a fixed schema with no free-form
  fields, and the client can turn it off
  ([ADR 0009](docs/adr/0009-no-vendor-access-to-client-data.md),
  [ADR 0026](docs/adr/0026-install-registry-and-heartbeats.md)).
- **The build enforces the boundary.** Code lives with the product that owns it, and Bazel
  visibility fails the build if the corp portal depends on client-side code
  ([ADR 0011](docs/adr/0011-ownership-based-layout.md)).
- **Models run in the client's own cloud.** AI features call the install's model service, off
  until the client turns them on
  ([ADR 0048](docs/adr/0048-awzy-bot-on-the-installs-own-model-service.md)).

## How it runs

- **As little stateful software in the client's cluster as possible.** Postgres, Kafka, and
  Valkey are the cloud's managed services; only ClickHouse runs in the cluster, because the
  product depends on it. Whatever awzy runs there has to recover by itself or be fixable from a
  runbook, since awzy can't log in
  ([ADR 0005](docs/adr/0005-managed-data-stores-clickhouse-in-cluster.md)).
- **Kafka is the record; ClickHouse is a view of it.** Decisions never wait on ClickHouse, and a
  lost ClickHouse refills from Kafka
  ([ADR 0006](docs/adr/0006-decision-log-through-kafka-into-clickhouse.md)).
- **Cloud differences live in two places:** the per-cloud Terraform modules and the `libs/cloud`
  adapters. Services and the bootstrap module never branch on the cloud
  ([ADR 0015](docs/adr/0015-cloud-agnostic-install.md)).
- **Push changes; don't poll.** PDPs wait to be told of policy, principal, and resource changes
  instead of checking on a timer, so a change reaches decisions as soon as it's made
  ([ADR 0077](docs/adr/0077-pdps-wait-for-changes-instead-of-polling.md)).
- **Deploys are zero-downtime.** A release never fails a check or a tool call, and migrations run
  from release Jobs before services roll
  ([ADR 0072](docs/adr/0072-deploys-are-zero-downtime.md),
  [ADR 0083](docs/adr/0083-postgres-migrations-run-from-a-release-job.md)).

## Trust in awzy itself

- **Anything that changes enforcement is signed.** Policy bundles, enforcement modes, and release
  images. Verifiers need only a public key and never apply something older than what they've
  applied ([ADR 0021](docs/adr/0021-signed-policy-bundles.md),
  [ADR 0057](docs/adr/0057-awzy-namespaces-run-only-signed-images-from-the-release-registry.md),
  [ADR 0080](docs/adr/0080-signed-enforcement-modes-and-grants-per-policy.md)).
- **There's a way back that doesn't depend on awzy.** Break-glass rollback is signed by admin
  keys enrolled at install, independent of the IdP, the PDP, and the portal
  ([ADR 0022](docs/adr/0022-break-glass-rollback.md)).
- **awzy is its own first client.** The portal is authorized by the PDP like any other app, and
  awzy's own MCP endpoints record what agents do as activity
  ([ADR 0020](docs/adr/0020-the-portal-is-authorized-by-the-pdp.md),
  [ADR 0084](docs/adr/0084-awzys-own-surfaces-record-agent-activity.md)).
- **Services authenticate to each other** with short-lived Kubernetes service-account tokens
  ([ADR 0055](docs/adr/0055-services-authenticate-with-kubernetes-service-account-tokens.md)).

## How the code is built

- **One domain layer, many surfaces.** GraphQL, MCP, and Awzy Bot call the same functions, so no
  tool can see or do more than the matching portal action
  ([platform plan](docs/plans/platform-plan.md#rules-for-awzys-own-mcp-servers)).
- **One commit, one version.** Every image, the chart, and the Terraform modules of a release come
  from one tested commit ([platform plan](docs/plans/platform-plan.md#bazel-monorepo)).
- **One word for each thing.** QIDs for awzy's IDs, UTC for time, `human` and `principal` in
  names, and an `Actor` for who did something
  ([ADR 0013](docs/adr/0013-qids-and-utc-timestamps.md),
  [ADR 0060](docs/adr/0060-actors-are-typed-entity-refs-not-strings.md),
  [ADR 0074](docs/adr/0074-a-human-is-named-human-everywhere.md)). The details are in
  [AGENTS.md](AGENTS.md#conventions).
- **Demo code stays in `authz/demo`.** Nothing a real install runs depends on it
  ([ADR 0063](docs/adr/0063-demo-only-code-in-authz-demo.md)).
