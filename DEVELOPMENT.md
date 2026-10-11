# Developing awzy

How to build, test, and run awzy on your machine. What awzy is and how the repository is laid
out: [README.md](README.md). Conventions for code and commits: [AGENTS.md](AGENTS.md).

## Setup

Once per machine:

- [bazelisk](https://github.com/bazelbuild/bazelisk), installed as `bazel` (`brew install
  bazelisk` puts both `bazel` and `bazelisk` in `/opt/homebrew/bin`). It runs the Bazel version in
  `.bazelversion`, and Bazel downloads the Rust, Node, and pnpm toolchains it builds with. Running
  it through `npx @bazel/bazelisk` works for builds but not for the pre-commit hook, which needs
  `bazel` or `bazelisk` on `PATH`.
- Docker, for the [local stack](#local-stack) and for trying images.
- Node and pnpm 9 (pinned in `package.json`; `corepack enable` picks it up), for the linters and
  the Vite dev servers.
- [Local HTTPS](#local-https) with mkcert, if you'll sign in to a portal.

Once per clone:

```sh
pnpm install   # oxlint, oxfmt, the frontends' packages, and turns on the pre-commit hook
```

`pnpm install` runs `git config core.hooksPath tools/git-hooks` (the `prepare` script in
`package.json`), which applies to every worktree of the clone. Check it with
`git config core.hooksPath`.

Then turn on Bazel's remote cache, so a build reuses what any clone on the same platform
already compiled ([ADR 0036](docs/adr/0036-buildbuddy-remote-cache.md)). CI builds on Linux, so
a Mac mostly reuses its own earlier builds, which still matters: a clippy run on a few crates
took over ten minutes without it and about 30 seconds with it. Put your API key from BuildBuddy
(org `awzy`) in `.bazelrc.user`, which is gitignored:

```sh
printf 'build --config=remote\nbuild --remote_header=x-buildbuddy-api-key=%s\n' "<key>" > .bazelrc.user
chmod 600 .bazelrc.user
```

A clone that already has one can share it: `cp ../other-clone/.bazelrc.user .`. Builds print
`Streaming build results to: https://app.buildbuddy.io/...` when it's on.

The pre-commit hook refuses a commit whose staged Rust or TypeScript isn't formatted, or whose
TypeScript fails oxlint's quick rules. It checks the staged content, not the working tree, and
uses rustfmt from Bazel's toolchain if none is installed. Clippy and type-aware oxlint need a
full build, so they're left to CI. `tools/fmt` fixes what the hook complains about; skip it once
with `git commit --no-verify`.

If the hook stops a commit:

| It says | Fix |
| --- | --- |
| `neither rustfmt nor bazel is on PATH` | `brew install bazelisk`, then check `which bazel` in the shell or tool that commits. Git GUIs and editors may not read your shell profile; give them a `PATH` that includes `/opt/homebrew/bin`. |
| `oxfmt and oxlint aren't installed` | `pnpm install` at the repository root. |
| `fix these, stage the changes, and commit again` | `tools/fmt` for formatting, or fix the oxlint findings it printed, then `git add` the files again. The hook checks what's staged. |

The first Rust commit in a clone takes a minute while Bazel builds rustfmt; later ones reuse it.

### Coding agents

The project skills live in `.agents/skills/`, which Codex reads; `.claude/skills` is a symlink to
it for Claude Code. Agents without skill support can read them as checklists. Each covers a
task that's easy to get half right:

| Skill | For |
| --- | --- |
| [`lint-fix`](.agents/skills/lint-fix/SKILL.md) | Running the linters and formatters and fixing what they report, before a commit or PR or when the hook or CI's lint step fails |
| [`local-stack`](.agents/skills/local-stack/SKILL.md) | Bringing up the [local stack](#local-stack), running the integration tests that otherwise skip, and starting services to try a change |
| [`new-service`](.agents/skills/new-service/SKILL.md) | Adding an `awzy-<name>` service through Cargo, Bazel, the heartbeat, the Helm chart, and the release registry |
| [`write-adr`](.agents/skills/write-adr/SKILL.md) | Recording an architecture decision in `docs/adr/` |

## Build and test

```sh
bazel test //...                                  # all Rust tests, Relay validation, typechecks
bazel build --config=lint //...                   # clippy and rustfmt checks; warnings fail (CI runs this)
tools/fmt                                         # format every Rust and TypeScript file
bazel build //authz/services/pdp:image_index               # linux/amd64 OCI image, built from macOS or Linux
```

TypeScript is linted with oxlint (type-aware rules run on tsgo, through oxlint-tsgolint) and
formatted with oxfmt, from the root `node_modules`:

```sh
pnpm lint                                         # oxlint --type-aware; warnings fail (CI runs this)
pnpm format                                       # oxfmt; `pnpm format:check` only checks
```

The protobuf APIs are checked with [buf](https://buf.build/docs/installation) (`brew install
bufbuild/buf/buf`), which CI's `proto` job runs
([ADR 0045](docs/adr/0045-sdks-are-enforcement-points-built-on-a-stable-proto.md)):

```sh
buf lint && buf format --diff --exit-code
buf breaking --against ".git#ref=origin/main,subdir=authz/libs/proto"   # after git fetch
```

Integration tests skip, and pass, unless the [local stack](#local-stack) is up and you add
`--config=local-stack`.

### Images

Every service gets `:image_index` (the release image) from the `awzy_service` macro. Linux
binaries are cross-compiled with a hermetic Zig C toolchain, so no Docker or Linux VM is needed
to build them. To try one locally (Docker running):

```sh
bazel run --platforms=//tools/platforms:linux_amd64 //authz/services/pdp:image_load
docker run --rm -p 8080:8080 awzy/awzy-pdp:dev
```

In production there is no separate frontend server: each portal backend serves its built
frontend (`AWZY_PORTAL_STATIC_DIR`) next to `/graphql`, and the portal image ships both.
`bazel build //portals/client/backend:image_index` builds that image.

### Dependencies

- Rust crates: declare them once in the root `Cargo.toml` `[workspace.dependencies]`, then
  `CARGO_BAZEL_REPIN=1 bazel test //...` to update `Cargo.lock` and `MODULE.bazel.lock`, then
  `bazel test //...` once without it: `MODULE.bazel.lock` records the variable, and a lockfile
  committed with it set changes on everyone's next build.
- Lockfile conflicts: `MODULE.bazel.lock` records a hash of every `Cargo.toml`, so any two
  branches that change a crate's dependencies conflict there. Nobody merges them by hand:
  - Locally, the merge driver keeps one side and the hooks re-pin and commit after a rebase or
    merge. If it stopped on another conflict, the commit that resolves it re-pins and includes
    the lockfiles (`tools/git-hooks`, installed by `pnpm install`).
  - On GitHub, after each push to main, `repin-prs` re-pins every open PR whose only conflict is
    the lockfiles, pushes the result, and starts its CI; a PR with other conflicts gets a comment
    asking for a rebase.
- npm packages: `pnpm install` updates `pnpm-lock.yaml`. pnpm runs with `hoist=false`, so every
  package declares every dependency it imports.
- IDE: `bazel run @rules_rust//tools/rust_analyzer:gen_rust_project`.

## Local stack

Postgres, ClickHouse, Kafka, and Valkey for development and integration tests:

```sh
docker compose -f deploy/local/compose.yaml up -d --wait
bazel test --config=local-stack //...     # database tests run instead of skipping
AWZY_LISTEN_ADDR=127.0.0.1:8081 AWZY_POSTGRES_URL=postgres://awzy:awzy@localhost:5432/awzy_policy \
  bazel run //authz/services/policy:awzy-policy

# ClickHouse tables (decision log), then a PDP that logs every decision through Kafka:
AWZY_CLICKHOUSE_URL=http://localhost:8123 AWZY_CLICKHOUSE_USER=awzy AWZY_CLICKHOUSE_PASSWORD=awzy \
  AWZY_CLICKHOUSE_DATABASE=awzy AWZY_CLICKHOUSE_KAFKA_BROKERS=kafka:29092 \
  bazel run //authz/jobs/clickhouse-migrate:awzy-clickhouse-migrate
AWZY_LISTEN_ADDR=127.0.0.1:8082 AWZY_KAFKA_BROKERS=localhost:9092 bazel run //authz/services/pdp:awzy-pdp
```

Each Postgres database has a generated `schema.sql` next to its `migrations/` (for example
[authz/services/context/schema.sql](authz/services/context/schema.sql)): its tables and indexes after every
migration, to read. After adding a migration, run `tools/db-schema` with the stack up and commit
the result; CI fails if one is stale. Explanations go in the migration, or in `COMMENT ON` to
show up in `schema.sql` too.

Migrations expand, then contract (ADR 0072), because during a rollout the old version runs
against the new schema. A migration may add tables, nullable columns or columns with a default,
and indexes. To drop, rename, or retype something, first ship a release that stops using it; the
next release's migration drops it, with the line marked `-- awzy: contract (stopped using in
<version>)`. `tools/ci/check-migrations.sh` checks every migration that isn't on `main` yet, in CI
and locally.

Each database test creates its own `test_*` schema. `docker compose -f deploy/local/compose.yaml down -v`
wipes everything.

## Local HTTPS

Local development uses HTTPS like a real install: the dev IdP serves it, awzy fetches signing keys
only over it, and the Vite dev servers use it ([ADR 0029](docs/adr/0029-https-in-local-development.md)).
mkcert makes a local certificate authority that only your machine trusts. Once per machine:

1. Install mkcert, and `nss` if you use Firefox (Firefox keeps its own trust store):

   ```sh
   brew install mkcert nss
   ```

2. Trust mkcert's CA. **Run this in a regular terminal**, not through Claude Code's `!` or another
   non-interactive shell: it asks for your password (sudo) to add the CA to the macOS keychain.
   Restart any open browser afterwards.

   ```sh
   mkcert -install
   ```

3. Make the certificate for `localhost`, `127.0.0.1`, and `::1` in `.dev-certs/` (gitignored; valid
   for about two years, rerun when it expires):

   ```sh
   mkdir -p .dev-certs && mkcert -cert-file .dev-certs/localhost.pem -key-file .dev-certs/localhost-key.pem \
     localhost 127.0.0.1 ::1
   ```

4. Check it, once the [dev IdP](#signing-in-locally) is running:

   ```sh
   curl --cacert "$(mkcert -CAROOT)/rootCA.pem" https://127.0.0.1:5556/.well-known/openid-configuration
   ```

What uses it:

| | URL | Trusts mkcert's CA through |
| --- | --- | --- |
| Dev IdP | `https://127.0.0.1:5556` | reads `.dev-certs/` itself |
| Client portal frontend (Vite) | `https://localhost:5173` | your browser (step 2) |
| Corp portal frontend (Vite) | `https://localhost:5174` | your browser (step 2) |
| Client portal backend, gateway | fetch keys from the dev IdP | `AWZY_AUTH_CA_FILE="$(mkcert -CAROOT)/rootCA.pem"` |

The backends themselves still listen on plain HTTP on loopback; the browser reaches them through
Vite. Quote `$(mkcert -CAROOT)`: on macOS the path contains a space. Tests and CI need none of this:
they use a self-signed certificate.

## Running the services

Every service listens on `AWZY_LISTEN_ADDR` (default `0.0.0.0:8080`, as in a cluster, where each
has its own pod). Locally they share one machine, so each gets its own port, and the commands
below use these:

| Port | Service |
| --- | --- |
| 8080 | client portal |
| 8081 | awzy-policy |
| 8082 | awzy-pdp |
| 8083 | awzy-challenge |
| 8084 | awzy-context |
| 8085 | awzy-intake |
| 8086 | awzy-gateway |
| 8087 | awzy-reasoner |
| 8088 | awzy-connectors |
| 8089 | awzy-reporter |
| 8090 | corp portal |
| 8095 | the synthetic company's MCP servers (larkspur-drive, larkspur-admin) |
| 5556 | the dev IdP |

### PDP

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8082 bazel run //authz/services/pdp:awzy-pdp   # policy decision point on :8082
```

Try a decision. Here an AI service account acts for bob, so the PDP checks both the account and bob
and the stricter verdict wins:

```sh
curl -s localhost:8082/v1/access/evaluation -H 'content-type: application/json' -d '{
  "principal": {"type": "ServiceAccount", "id": "sales-genie"}, "on_behalf_of": {"type": "Human", "id": "bob"},
  "action": "read", "resource": {"type": "Resource", "id": "design-doc"}, "context": {"risk_score": 90},
  "entities": [
    {"uid": {"type": "Human", "id": "bob"}, "attrs": {}, "parents": [{"type": "Group", "id": "engineering"}]},
    {"uid": {"type": "ServiceAccount", "id": "sales-genie"}, "parents": [{"type": "Group", "id": "engineering"}],
     "attrs": {"owner": {"__entity": {"type": "Human", "id": "bob"}}, "ai": true}},
    {"uid": {"type": "Resource", "id": "design-doc"}, "attrs": {"sensitivity": "internal"}, "parents": []}
  ]}'
# {"qid":"decision_0199…","verdict":"challenge","challenges":["verification","approval"],
#  "policies":["absent-human-approval","high-risk-verify"],
#  "on_behalf_of":{"verdict":"challenge","challenges":["verification"],"policies":["high-risk-verify"]},
#  "mode":"monitor"}
```

A service account is assumed to have no person present unless the request says
`"human_present": true`, so at this risk `absent-human-approval` fires too. Every challenge that
fires is required. `mode` is `monitor` until an admin switches the install to enforce (ADR 0007).

Principals are `Human` and `ServiceAccount` (any non-human identity, including AI agents; see
`policies/awzy.cedarschema`). A service account acting for a person gets at most what that person could do.

The same port serves gRPC (`awzy.authz.v1.Authorizer/Check`, defined in `authz/libs/proto/awzy/authz/v1/authz.proto`),
which is what enforcement points use. Rust bindings come from `//authz/libs/proto` (protox + tonic, no `protoc`).

### Freshness (how fast changes reach decisions)

`pdp_freshness` measures the platform plan's freshness targets against running services, under
load. It needs a PDP configured with awzy-policy, awzy-challenge, and awzy-context, and their
addresses. Use it on local stacks and test installs only: it publishes a bundle with probe policies
(restoring the original at the end), provisions a probe person and group over SCIM, contains and
lifts the probe, and relabels a probe document.

```sh
AWZY_PDP_ADDR=http://127.0.0.1:8082 AWZY_POLICY_ADDR=http://127.0.0.1:8081 \
  AWZY_CHALLENGE_ADDR=http://127.0.0.1:8083 AWZY_CONTEXT_URL=http://127.0.0.1:8084 AWZY_SCIM_TOKEN=... \
  bazel run -c opt //authz/services/pdp:pdp_freshness -- 5 16   # rounds, background clients
```

### Policy bundles

awzy-policy runs against the [local stack](#local-stack). Signed policy bundles are optional
locally (installs always sign; bootstrap makes the key): generate a key pair, then give
awzy-policy the private key and the PDP the public key. A PDP with a verify key refuses unsigned
or altered bundles and enforcement modes, and older ones than it has applied (ADR 0080).

```sh
openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt -out signing.pem
openssl ec -in signing.pem -pubout -out verify.pem
# awzy-policy: AWZY_POLICY_SIGNING_KEY_FILE=signing.pem   awzy-pdp: AWZY_POLICY_VERIFY_KEY_FILE=verify.pem
```

`awzy-publish` publishes the release's base bundle straight to awzy-policy:

```sh
bazel run //authz/services/policy:awzy-publish -- --endpoint http://127.0.0.1:8081 --by "$USER"
```

Break-glass rollback, for when a bad bundle locks admins out: enroll admins' public keys at install
(`AWZY_POLICY_BREAK_GLASS_KEYS_FILE`, one or more PEM keys), then an admin runs

```sh
bazel run //authz/services/policy:awzy-break-glass -- --endpoint http://127.0.0.1:8081 --key admin.pem \
  --reason "forbid-all bundle locked us out"      # optional: --to-version N
```

It talks to awzy-policy directly, so it works when the IdP, PDP, or portal is the problem. Requests
expire after 5 minutes, can't be replayed, and every use is recorded and logged.

### Challenges and containments (ADR 0054)

awzy-challenge runs against the [local stack](#local-stack) (its database, `awzy_challenge`, is made
by the stack's init script). It asks the PDP who may approve:

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8083 AWZY_POSTGRES_URL=postgres://awzy:awzy@localhost:5432/awzy_challenge \
  AWZY_PDP_ADDR=http://127.0.0.1:8082 bazel run //authz/services/challenge:awzy-challenge
# PDP and client portal: add AWZY_CHALLENGE_ADDR=http://127.0.0.1:8083
```

A PDP with `AWZY_CHALLENGE_ADDR` isn't ready until it has the containment list. Challenges are only
opened in enforce mode: switch the install on the portal's Enforcement page, then a challenged
request comes back with a `challenge_qid`. The person answers it on the Challenges page and retries.
Locally, `AWZY_CHALLENGE_GRANT_SECS` and `AWZY_CHALLENGE_PENDING_SECS` shorten how long a pass
lasts and how long a challenge waits.

Security keys (ADR 0059) need a relying party: add `AWZY_CHALLENGE_WEBAUTHN_RP_ID=localhost` and
`AWZY_CHALLENGE_WEBAUTHN_ORIGINS=https://localhost:5173` (the Vite dev server) to awzy-challenge.
Browsers only offer WebAuthn on a secure origin, and an RP ID must be a domain, so open the portal
at `localhost`, not `127.0.0.1`. Without a hardware key, Chrome's DevTools can add a virtual one
(More tools, WebAuthn).

### MCP gateway (first enforcement point)

`awzy-gateway` sits in front of MCP servers at `/mcp/{upstream}`. It validates the caller's bearer
token (RFC 8693 `act` claim = a service account acting for the token's subject), checks every
`tools/call` with awzy-pdp as action `call` on `McpTool::"<upstream>/<tool>"`, and in enforce mode
returns a denied call as an MCP tool error without forwarding it. Other messages pass through.

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8086 AWZY_GATEWAY_CONFIG=gateway.json AWZY_GATEWAY_DEV_SECRET=dev-secret \
  AWZY_PDP_ADDR=http://127.0.0.1:8082 bazel run //authz/services/gateway:awzy-gateway
```

```json
{
  "token": { "issuer": "https://idp.local", "audience": "awzy-gateway", "hs256_secret_env": "AWZY_GATEWAY_DEV_SECRET" },
  "upstreams": [
    { "name": "awzy-portal", "url": "http://127.0.0.1:8080/mcp", "default_sensitivity": "internal",
      "tools": { "propose_policy": "confidential" } }
  ],
  "service_accounts": [{ "id": "sales-genie", "owner": "bob", "ai": true, "groups": ["engineering"] }],
  "outage": { "fail_open_tiers": ["public", "internal"], "cached_allow_ttl_secs": 300 }
}
```

If the PDP can't be reached in enforce mode, the gateway replays only a recent allow, for tools
listed by name in a fail-open tier; everything else fails closed, and `confidential` and
`crown_jewels` always do ([ADR 0030](docs/adr/0030-gateway-outage-fallback.md)). Without `outage`
nothing fails open. With `AWZY_CHALLENGE_ADDR`, the gateway follows awzy-challenge's containments:
it forgets a contained principal's recent allows and never replays one for them, and until the first
list arrives it replays nothing.

Installs use `"rs256_public_key_file"` with the IdP's key instead of the HS256 dev secret.

A tool that acts on one resource can be checked on that resource too
([ADR 0050](docs/adr/0050-mcp-tool-calls-checked-on-the-resource.md)): list it under
`resource_tools` with the argument holding the resource's ID, and the gateway also checks `read`
(or `write`) on `Resource::"<id>"`, which the PDP loads from awzy-context. To try it with the
synthetic company, run larkspur-drive (an MCP server over awzy-context's documents) and add it
as an upstream:

```sh
AWZY_CONTEXT_URL=http://127.0.0.1:8084 bazel run //authz/demo/synthetic:awzy-synthetic -- drive   # :8095/mcp
```

```json
{ "name": "larkspur-drive", "url": "http://127.0.0.1:8095/mcp", "default_sensitivity": "internal",
  "tools": { "read_document": "internal", "list_folder": "internal" },
  "resource_tools": { "read_document": { "argument": "id" }, "list_folder": { "argument": "id" } } },
{ "name": "larkspur-admin", "url": "http://127.0.0.1:8095/admin/mcp", "default_sensitivity": "crown_jewels",
  "tools": { "reset_password": "confidential", "disable_mfa": "crown_jewels",
             "rotate_admin_credentials": "crown_jewels", "export_directory": "crown_jewels" } }
```

The same process serves larkspur-admin at `/admin/mcp`: IT's high-risk operations, which change
nothing ([ADR 0064](docs/adr/0064-unfamiliar-territory-raises-risk-and-high-risk-operations-need-a-key.md)).

Then make Larkspur's everyday activity flow through it (demo plan step 5b): people opening their
work's and their team's documents, SalesGenie acting for Nora, FinanceSummarizer alone at 6am,
with a little noise. Every call is a PDP decision you'll see in the client portal's Decisions
page. Run the gateway with `"service_account_registry": "awzy-context"` and the PDP with
`AWZY_CONTEXT_ADDR`, so Larkspur's people, agents, and documents come from awzy-context:

```sh
AWZY_GATEWAY_URL=http://127.0.0.1:8086 AWZY_GATEWAY_DEV_SECRET=dev-secret \
  bazel run //authz/demo/synthetic:awzy-synthetic -- activity --rate 20 --days 1
```

The driver signs each call's token with the gateway's dev secret (issuer `https://idp.local`,
audience `awzy-gateway`; override with `AWZY_TOKEN_ISSUER` and `AWZY_TOKEN_AUDIENCE`).

The first day includes the demo plan's scenarios: the curious contractor, the rogue agent's
first half, the suddenly foolish administrator, and the peer outlier. awzy catches the last two
only if awzy-reasoner scores the morning before what they do next, a few hundred calls later,
so run the reasoner with a short `AWZY_REASONER_INTERVAL_SECS` (30) and the history below. The rogue
agent's ending also needs the plan's owner to reject SalesGenie's request on the client portal's
Challenges page, Nora's export sent to awzy-intake (set `AWZY_INTAKE_URL` and `AWZY_INTAKE_TOKEN`
for `activity`), and automated containments switched to enforce on the Enforcement page; in
monitor, she's listed as "would contain" instead. The run ends with a `detection` line: scenario steps caught, and ordinary
accesses challenged. Switch the install to enforce first, because in monitor mode the gateway
allows everything and nothing is caught. Leave them out with `--no-scenarios`. To run them end
to end without any of this, `bazel test --config=local-stack //authz/demo/synthetic:synthetic_test`.

For baselines, send a backdated month of the same activity to awzy-intake (demo plan step 5c):

```sh
AWZY_INTAKE_URL=http://127.0.0.1:8085 AWZY_INTAKE_TOKEN=... \
  bazel run //authz/demo/synthetic:awzy-synthetic -- history --days 30
```

### Signing in locally

The client portal needs an IdP. `tools/dev-idp` is a development-only OIDC provider with a
"sign in as" page (alice in `security`, bob in `engineering`, carol in `finance`) and no passwords.
Never deploy it. Set up [local HTTPS](#local-https) first.

```sh
bazel run //tools/dev-idp:awzy-dev-idp           # https://127.0.0.1:5556
AWZY_LISTEN_ADDR=127.0.0.1:8080 AWZY_POLICY_ADDR=http://127.0.0.1:8081 AWZY_PDP_ADDR=http://127.0.0.1:8082 \
  AWZY_AUTH_ISSUER=https://127.0.0.1:5556 AWZY_AUTH_JWKS_URL=https://127.0.0.1:5556/jwks \
  AWZY_AUTH_CA_FILE="$(mkcert -CAROOT)/rootCA.pem" \
  AWZY_CLICKHOUSE_URL=http://localhost:8123 AWZY_CLICKHOUSE_USER=awzy AWZY_CLICKHOUSE_PASSWORD=awzy \
  bazel run //portals/client/backend:awzy-client-portal
```

Then `cd portals/client/frontend && pnpm dev` and open https://localhost:5173.

Without `.dev-certs/` the dev IdP makes a self-signed certificate; set `AWZY_DEV_IDP_CA_OUT` to a
path to get its PEM for `AWZY_AUTH_CA_FILE`. Browsers won't trust it, so use mkcert for anything
interactive.

A JWKS URL must be `https://`, with no exception for localhost. For an IdP behind a private CA
(or the dev IdP), set `AWZY_AUTH_CA_FILE` to its PEM certificates.

Every portal and MCP operation is checked with the PDP as `view` or `administer` on a `PortalArea`
([ADR 0020](docs/adr/0020-the-portal-is-authorized-by-the-pdp.md)). The base policies let the
`security` group do both, so sign in as alice to change things. If the active bundle predates
`PortalArea`, nobody can use the portal until a newer one is published with
[`awzy-publish`](#policy-bundles).

Agents get tokens from the dev IdP's token endpoint: `grant_type=client_credentials&client_id=<account>`
for a service account on its own authority, or RFC 8693 token exchange
(`subject_token=<person's token>&actor=<account>`) for an account acting for a person. Add
`audience=awzy-mcp` for the portal's `/mcp` (`AWZY_MCP_AUDIENCE`) or `audience=awzy-gateway` for
awzy-gateway: each takes tokens for its own audience, and the portal's GraphQL API only `awzy`
([ADR 0081](docs/adr/0081-agents-get-tokens-of-their-own-and-say-which-client-they-are.md)). Register the
account in `AWZY_SERVICE_ACCOUNTS_FILE` (a JSON list of `{id, owner, ai, groups}`).

### People and groups from the IdP (ADR 0031)

`awzy-context` takes people and groups from the client's IdP over SCIM 2.0 at `/scim/v2`, and
PDPs load them from it, so a group change or deactivation in the IdP applies within seconds,
whatever older tokens say. Publish the current base bundle first (its schema has `Human.active`).

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8084 AWZY_POSTGRES_URL=postgres://awzy:awzy@localhost:5432/awzy_context \
  AWZY_SCIM_TOKEN=$(openssl rand -hex 24) bazel run //authz/services/context:awzy-context
# PDP: add AWZY_CONTEXT_ADDR=http://127.0.0.1:8084
curl -X POST http://127.0.0.1:8084/scim/v2/Users -H "authorization: Bearer $AWZY_SCIM_TOKEN" \
  -H 'content-type: application/scim+json' -d '{"userName": "bob", "active": true}'
```

The client portal's Directory page (`AWZY_CONTEXT_ADDR` on the portal) shows the people and
groups the IdP provisioned and registers service accounts ([ADR 0033](docs/adr/0033-service-account-registry-in-awzy-context.md)).
With awzy-context, the PDP denies any service account that isn't registered there; set
`AWZY_SERVICE_ACCOUNT_REGISTRY=awzy-context` on the portal and `"service_account_registry": "awzy-context"`
in the gateway's config to drop their own lists.

In the IdP, add awzy as a SCIM app with base URL `https://<install>/scim/v2` and that token.
A person's ID is their SCIM `userName`; if the IdP's tokens identify people by another claim than
`sub`, set it (`AWZY_AUTH_SUBJECT_CLAIM=email` in the client portal, `subject_claim` in the
gateway's token config).

### The synthetic company (ADR 0039)

`awzy-synthetic` provisions Larkspur Capital, the synthetic company in
[docs/demo-plan.md](docs/demo-plan.md), into awzy-context the way an IdP would: about 200
people, their teams and departments, contractors, and four AI service accounts (SalesGenie,
CodeHelper, SupportBot, FinanceSummarizer). `world` then writes its folders, documents,
repositories, and work items through awzy-context's batch API, and `all` does both. The same
seed always gives the same company; running it again replaces rather than duplicates.

```sh
# With awzy-context running as above (same AWZY_SCIM_TOKEN):
AWZY_CONTEXT_URL=http://127.0.0.1:8084 AWZY_SCIM_TOKEN=... \
  bazel run //authz/demo/synthetic:awzy-synthetic -- all --profile demo --seed 1
bazel run //authz/demo/synthetic:awzy-synthetic -- directory --print   # just print it
```

`--profile scale` makes about 10,000 people, 200 agents, and a million documents, for the scale
tests. Demo installs
run it as a Job after each sync (`demoData.enabled` in the chart, `demo_data` in bootstrap).

### Google Drive (ADR 0069)

awzy-connectors syncs a real Workspace's shared drives into awzy-context. It needs a Google
service account key with domain-wide delegation (see
[the connector's README](authz/services/connectors/README.md)), and awzy-context running:

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8088 AWZY_CONTEXT_URL=http://127.0.0.1:8084 \
  AWZY_CONNECTORS_DRIVE_KEY_FILE=$PWD/drive-key.json AWZY_CONNECTORS_DRIVE_SUBJECT=awzy-reader@example.com \
  bazel run //authz/services/connectors:awzy-connectors
curl -s 'http://127.0.0.1:8084/v1/resources?source=google_drive&limit=5'
```

Keep the key file out of the repository.

Jira (ADR 0070) runs in the same process, with an Atlassian API token in a file:

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8088 AWZY_CONTEXT_URL=http://127.0.0.1:8084 \
  AWZY_CONNECTORS_JIRA_URL=https://example.atlassian.net AWZY_CONNECTORS_JIRA_USER=awzy@example.com \
  AWZY_CONNECTORS_JIRA_TOKEN_FILE=$PWD/jira-token AWZY_CONNECTORS_JIRA_PROJECTS=FIN,ENG \
  bazel run //authz/services/connectors:awzy-connectors   # add the Drive variables to run both
curl -s 'http://127.0.0.1:8084/v1/work-items?source=jira&limit=5'
```

### Events (awzy-intake)

`awzy-intake` takes app and audit-log events, normalizes them, and puts them on Kafka, from
where ClickHouse's `events` table picks them up (run the ClickHouse migrations from
[Local stack](#local-stack) first). Senders authenticate with `AWZY_INTAKE_TOKEN`.

```sh
export AWZY_INTAKE_TOKEN=$(openssl rand -hex 24)
AWZY_LISTEN_ADDR=127.0.0.1:8085 AWZY_KAFKA_BROKERS=localhost:9092 bazel run //authz/services/intake:awzy-intake
curl -X POST http://127.0.0.1:8085/v1/events -H "authorization: Bearer $AWZY_INTAKE_TOKEN" \
  -H 'content-type: application/json' -d '{"events": [{"at": "'"$(date -u +%FT%TZ)"'", "system": "jira",
  "principal": {"type": "Human", "id": "bob"}, "action": "read", "resource": {"type": "Ticket", "id": "ENG-1"}}]}'
# {"accepted":1,"qids":["event_0199…"]}
```

A CloudTrail log file goes to `/v1/events/aws-cloudtrail` as is (`{"Records": [...]}`, after
gunzipping). The events show up in ClickHouse within a few seconds:
`SELECT at, principal_id, action, resource_id FROM awzy.events ORDER BY at DESC LIMIT 10`.

### Models for Awzy Bot (ADR 0048)

`libs/llm` calls Claude on Amazon Bedrock with the standard AWS credential chain; tests use a
scripted fake. To check Bedrock access for real (it bills the account), use a profile whose
account has Claude enabled in Bedrock (Bedrock's one-time Anthropic use-case form, under Model
access). Bazel's test sandbox can't refresh an SSO token, so pass short-lived credentials in:

```sh
eval "$(aws configure export-credentials --profile awzy-example-install --region us-east-2 --format env)"
AWS_REGION=us-east-2 AWZY_TEST_BEDROCK_MODEL=anthropic.claude-opus-5-5 bazel test //libs/llm:llm_test \
  --test_env=AWS_ACCESS_KEY_ID --test_env=AWS_SECRET_ACCESS_KEY --test_env=AWS_SESSION_TOKEN \
  --test_env=AWS_REGION --test_env=AWZY_TEST_BEDROCK_MODEL --test_output=all --cache_test_results=no
```

The client portal turns Awzy Bot on with `AWZY_BOT_ENABLED=true` (plus `AWS_REGION` and AWS
credentials, e.g. `AWS_PROFILE`); the Policies page then shows "Ask Awzy Bot". Without Bedrock
access its answers are an error naming the reason. Services pick the model with `AWZY_LLM_MODEL` (default `anthropic.claude-opus-5-5`; some accounts
need a regional inference profile such as `us.anthropic.claude-opus-5-5`) and `AWZY_LLM_EFFORT`.

### Fleet reporting (ADR 0026)

Each install's `awzy-reporter` sends a heartbeat (versions, readiness, a few aggregates; nothing
free-form) to the corp portal every minute. Register the install in the corp portal first
(`registerInstall`), which returns its install ID and a reporting token, then:

```sh
AWZY_LISTEN_ADDR=127.0.0.1:8089 AWZY_INSTALL_ID=install_… AWZY_REPORTING_TOKEN=awzyrt_… \
  AWZY_REPORTER_CORP_PORTAL_URL=http://127.0.0.1:8090 \
  AWZY_REPORTER_TARGETS=awzy-pdp=http://127.0.0.1:8082,awzy-policy=http://127.0.0.1:8081 \
  bazel run //authz/services/reporter:awzy-reporter            # add -- --print to see one without sending
```

For ClickHouse disk use and Kafka lag (ADR 0042), add `AWZY_CLICKHOUSE_URL=http://localhost:8123`,
`AWZY_CLICKHOUSE_PASSWORD`, and `AWZY_KAFKA_BROKERS=localhost:9092`. `AWZY_CLICKHOUSE_USER`
defaults to `awzy_reporter`, which `awzy-clickhouse-migrate` creates when it's given
`AWZY_CLICKHOUSE_REPORTER_PASSWORD`; locally, `AWZY_CLICKHOUSE_USER=awzy` with password `awzy`
works too.

In a cluster the Helm chart runs the reporter (`reporter.installId`, and the reporting token in the
`awzy-reporting-token` Secret); it reads awzy's workloads through a read-only Role in each awzy
namespace and scrapes the PDPs' `/metrics`. Its ClickHouse password is the
`awzy-clickhouse-reporter` Secret that bootstrap makes. `reporter.enabled=false` deploys nothing.

### Metrics and dashboards (ADR 0068)

Every service serves Prometheus metrics at `/metrics` on its own port: requests by route with
latency, readiness, its release, and its own numbers (the PDP's checks by verdict and latency).
To watch them in Grafana, start the services on the ports above, then:

```sh
docker compose -f deploy/local/compose.yaml --profile observability up -d
open http://localhost:3000   # Grafana, dashboard "awzy: operator"; Prometheus is on :9091
```

The dashboards and alert rules are in `deploy/observability/`. A new metric goes in the service
as an `awzy_server::metrics::Source`, named `awzy_<service>_<what>_<unit>`, and never with an
principal's, resource's, or policy's ID as a label.

### Corp portal (demos and testing only)

The corp portal signs staff in with usernames and passwords
([ADR 0025](docs/adr/0025-corp-portal-passwords-for-demos-and-testing.md)). This is a stopgap
until awzy has its own IdP; don't build on it.

```sh
echo 'a long password' | bazel run //portals/corp/backend:awzy-corp-user -- --username max --name Max
# put the printed object in a JSON list in accounts.json, then:
AWZY_LISTEN_ADDR=127.0.0.1:8090 AWZY_CORP_PORTAL_ACCOUNTS_FILE=$PWD/accounts.json \
  AWZY_CORP_PORTAL_SESSION_SECRET=$(openssl rand -hex 32) AWZY_CORP_PORTAL_INSECURE_COOKIES=1 \
  bazel run //portals/corp/backend:awzy-corp-portal
```

`AWZY_CORP_PORTAL_INSECURE_COOKIES=1` drops the cookie's `Secure` flag for plain-HTTP local runs; never
set it on a deployment.

Fleet alerts (ADR 0043) are logged when an install becomes degraded, stops reporting, or
recovers. To send them to Slack, set `AWZY_CORP_PORTAL_ALERTS_SLACK_WEBHOOK_URL` to an incoming webhook, and
`AWZY_CORP_PORTAL_PUBLIC_URL` (e.g. `http://127.0.0.1:8090`) for the link in each message.

The corp portal needs Postgres for its registry: `AWZY_POSTGRES_URL=postgres://awzy:awzy@localhost:5432/awzy_corp`
(created by the compose stack's init script; on an older volume, `CREATE DATABASE awzy_corp`).

## Frontends

```sh
bazel run //portals/client/frontend:update_schema   # regenerate schema.graphql from the Rust backend
cd portals/client/frontend && pnpm relay && pnpm dev # https://localhost:5173 (with .dev-certs/), proxies /graphql and /auth/config to :8080
```

`bazel test` fails if `schema.graphql` or the committed Relay artifacts are stale. On a pull
request you needn't regenerate them: the `schema-prs` workflow (`tools/ci/regen-schemas.sh`) does,
pushes them to the branch, and starts CI again.

`//portals/client/e2e:signin_test` signs in to the client portal in headless Chrome through the dev
IdP (code + PKCE), switches users, and checks what the backend sees. It uses the machine's Chrome
(`CHROME_PATH`, or the usual install locations) and skips if there is none.

To work on a frontend against a deployed portal instead of a local backend
([ADR 0024](docs/adr/0024-local-frontend-against-a-deployed-backend.md)):

```sh
AWZY_BACKEND=https://demo.awzysecurity.com VITE_AWZY_CLIENT_ID=awzy-client-portal-dev pnpm dev
```

Vite proxies `/graphql` and `/auth/config` there, so the browser sees one origin and the deployed
backend needs no CORS. You sign in with that install's IdP, and the backend authorizes you as it
would anyone else. The IdP needs a separate client (`VITE_AWZY_CLIENT_ID`) that allows
`https://localhost:5173/auth/callback` as a redirect URI (`http://` without the mkcert files). Only register it for our own staging and
demo installs. A client's install shouldn't accept localhost redirects.

The corp frontend works the same way (`cd portals/corp/frontend`, Vite on :5174, local backend on
:8090), against awzy's own corp deployments. It needs no IdP client: corp sign-in is a session
cookie, which passes through the proxy.
