<p align="center"><img src="docs/brand/kangaroo.svg" alt="awzy's kangaroo" width="120"></p>

# awzy

Beyond Zero authorization for the AI era: per-action, per-resource decisions for humans and
agents, deployed single-tenant inside each client's cloud (AWS or Google Cloud; Azure is planned).

**Try it:** [aws.demo.awzysecurity.com](https://aws.demo.awzysecurity.com), a live install with a
synthetic 200-person company. Sign in as any sample person (no password) and look around; it's
read-only, so changes are turned off. Or run it
on your machine: `tools/demo up` ([below](#run-it-locally)).

## What it does

Zero Trust checks who you are when you connect. Google's [Beyond Zero](docs/beyond-zero.pdf)
paper argues that isn't enough once AI agents act for people: every action on every resource
should be decided on its own, with what's known right then about the person, the agent, the
resource, and the moment. awzy implements it.

- **Every action is a decision.** An agent reading a document, a person exporting a report, an
  admin changing a role: each is checked against Cedar policies by a policy decision point, and
  logged with the policies and attributes that decided it. A decision takes about half a
  millisecond of CPU. Pushed to their limit, the two-node example installs decide 4,500 to
  5,800 checks a second, at p99 9 ms on Google Cloud and 14 ms on AWS's burstable nodes
  ([measurements](docs/perf/2026-10-12-hot-path.md)).
- **Agents never exceed the person they act for.** An agent acting for someone is checked as
  itself and as them, and the stricter verdict wins.
- **More than allow and deny.** A risky request can be *challenged* instead: a justification, a
  touch of a security key, or a named approver, after which the request goes through.
- **awzy brings the context.** People and groups arrive from your IdP over SCIM. Documents,
  sharing, and work come from connectors (Google Drive, Jira), classified by sensitivity, with
  the reasons. Risk scores and model-written investigations come from what people and agents
  actually do.
- **Monitor before you enforce.** A new install decides everything but blocks nothing, and shows
  what it would have denied or challenged, until you switch it on.
- **It runs in your cloud, not ours.** Each client gets their own install in their own account.
  awzy's staff have no access to it. Support sessions are approved by the client each time,
  read-only, and expire.

The first enforcement point is a gateway in front of MCP servers, so agents' tool calls are
decided before they run.

## How it compares

Policy engines such as OPA, Cedar, and Cerbos answer "may this principal do this?" from the
attributes you give them, with allow or deny. awzy uses Cedar as its policy language, and adds
what a policy engine leaves to you:

- the attributes: who someone is, what a resource is and how sensitive, and current risk
- outcomes between allow and deny: challenges, approvals, and automated containment
- delegation, so an agent acting for a person is held to both
- an enforcement point for AI agents' tool calls, and a portal to run it all

## Run it locally

```sh
tools/demo up       # Docker and Bazel (bazelisk); the first build takes a while
```

It starts Postgres, Kafka, and ClickHouse in Docker and awzy's services from source, and
provisions Larkspur Capital, the synthetic company. Then it replays a day of the company's work
through the MCP gateway. Open http://127.0.0.1:8080 and sign in as `alice` (the security team).
`tools/demo check` says whether everything's answering, and `tools/demo down` stops it.

## Status

awzy is pre-release. The AWS example install (the demo above) runs end to end, and a Google
Cloud one is being built. It hasn't yet been run against a client's own IdP. See the
[platform plan](docs/plans/platform-plan.md) for what's built and what's next. To report a
vulnerability, see [SECURITY.md](SECURITY.md).

## Read more

- [docs.awzysecurity.com](https://docs.awzysecurity.com): the public documentation, built from [`docs/site`](docs/site)
- [Principles](PRINCIPLES.md): the rules the design follows, and the decisions behind them
- [Platform plan](docs/plans/platform-plan.md): the architecture, and what's built so far
- [Deployment plan](docs/plans/deployment-plan.md): how installs reach a client's cloud
- [Demo plan](docs/plans/demo-plan.md): the synthetic company the demo and tests run on
- [Decision records](docs/adr/): one file per architecture decision
- [Beyond Zero](docs/beyond-zero.pdf), the paper awzy implements (Valente and Zalewski, ACM 2026, CC BY 4.0)

## Layout

| Path | What | Runs in |
| --- | --- | --- |
| [`authz/`](authz/README.md) | [`services/`](authz/services) ([pdp](authz/services/pdp/README.md), [gateway](authz/services/gateway/README.md), [policy](authz/services/policy/README.md), [context](authz/services/context/README.md), [connectors](authz/services/connectors/README.md), [intake](authz/services/intake/README.md), [reasoner](authz/services/reasoner/README.md), [challenge](authz/services/challenge/README.md), [reporter](authz/services/reporter/README.md)), [`jobs/`](authz/jobs) ([clickhouse-migrate](authz/jobs/clickhouse-migrate/README.md)), [`demo/`](authz/demo) ([synthetic](authz/demo/synthetic/README.md): the demo's synthetic company), [`libs/`](authz/libs) ([classify](authz/libs/classify/README.md), [identity](authz/libs/identity/README.md), [proto](authz/libs/proto/README.md), [service-auth](authz/libs/service-auth/README.md), [signing](authz/libs/signing/README.md)) | client cluster |
| [`portals/client/`](portals/client/README.md) | [`backend`](portals/client/backend) (GraphQL at `/graphql`, MCP at `/mcp`, serves the frontend), [`frontend`](portals/client/frontend) (Vite + React + Relay + Mantine) | client cluster |
| [`portals/corp/`](portals/corp/README.md) | same shape, for awzy staff; never holds client data; plus [`registry`](portals/corp/registry/README.md), the release registry proxy | vendor account |
| [`portals/common/`](portals/common/README.md) | TypeScript packages both portal frontends use: [`@awzy/ui`](portals/common/ui), [`@awzy/relay`](portals/common/relay) |  |
| [`libs/`](libs) | shared Rust crates: [model](libs/model), [server](libs/server), [telemetry](libs/telemetry), [cloud](libs/cloud), [llm](libs/llm), [store](libs/store), [portal](libs/portal), [heartbeat](libs/heartbeat) |  |
| [`policies/`](policies) | base Cedar [schema](policies/awzy.cedarschema) and [policies](policies/base.cedar) |  |
| [`docs/`](docs) | the plans above and the [decision records](docs/adr) |  |
| [`tools/`](tools) | Bazel macros ([`awzy_service`](tools/rust.bzl), [`awzy_rust_library`](tools/rust.bzl)), the [`client_side`](tools/BUILD.bazel) visibility group, and CI scripts ([`tools/ci/`](tools/ci)) |  |
| [`deploy/`](deploy) | the [Helm chart](deploy/helm/awzy), [Terraform](deploy/terraform) for installs and the vendor side, and the [local stack](deploy/local/compose.yaml) |  |

`portals/corp` cannot depend on `authz/` or `libs/store`. Bazel visibility (`//tools:client_side`)
fails the build if it tries.

## Development

[DEVELOPMENT.md](DEVELOPMENT.md) covers setup (including the pre-commit hook), building and
testing, the local stack, local HTTPS, and running each service and portal. Coding agents also
read [AGENTS.md](AGENTS.md).

## Installing

[INSTALL.md](INSTALL.md) is the runbook for installing awzy in your own cloud account: the cloud
layer, bootstrap, DNS and your IdP, and checking the install.

## License

[MIT](LICENSE).
