# Installing awzy

How to install awzy in your own cloud account. Developing awzy is in
[DEVELOPMENT.md](DEVELOPMENT.md); why an install looks the way it does is in
[docs/plans/deployment-plan.md](docs/plans/deployment-plan.md).

> **Tested on 2026-10-11** by the example client's install
> ([deploy/terraform/examples/example-client](deploy/terraform/examples/example-client/README.md)),
> the worked example throughout: every step below was run end to end on AWS with the demo IdP,
> and step 7's checks pass. A real IdP's sign-in and SCIM (step 6) haven't been run yet.

An install runs entirely in your account. awzy (the vendor) holds no credential for it: the
install pulls signed releases from awzy's registry and sends a heartbeat out, and nothing comes
in. AWS is the only cloud so far.

It has two layers, applied in order:

| Layer | Module | What it makes |
| --- | --- | --- |
| Cloud | One per cloud: [`deploy/terraform/aws/install`](deploy/terraform/aws/install) on AWS | The network, a Kubernetes cluster, managed Postgres, Kafka, and Valkey, a bucket, a key for data at rest and one awzy-policy signs policy with, and workload identities. Outputs `install_contract`. On AWS: VPC, EKS, Aurora, MSK, ElastiCache, S3, KMS, and Pod Identity. |
| Cluster | [`deploy/terraform/bootstrap`](deploy/terraform/bootstrap) | Namespaces, Secrets, Argo CD, Kyverno, External Secrets, the ClickHouse operator, Envoy Gateway, cert-manager, and metrics-server, then an Argo CD Application for the awzy chart. Never cloud-specific. |

You call both from your own Terraform, as
[`examples/example-client`](deploy/terraform/examples/example-client) does. On a cluster you
already have, you make the cloud resources yourself, write `install_contract` to match, check the
cluster with the preflight in step 5, and apply only the cluster layer.

## 1. Before you start

- **Tools:** OpenTofu ≥ 1.10 (or Terraform), kubectl, and your cloud's CLI.
- **A cloud account for the install**, separate from the one that holds Terraform state. The
  example client keeps state in its AWS management account and installs into a member account
  (`awzy-example-install`).
- **A region with two zones.**
- **A hostname** for the client portal (`awzy.example.com`) in DNS you control, and an email
  address for Let's Encrypt.
- **On AWS:**
  - The install works under SCPs that deny other Regions and deny IAM users with access keys; it
    uses neither.
  - Some new accounts can't create load balancers until AWS Support allows it
    (`OperationNotPermitted: This AWS account currently does not support creating load
    balancers`, on the Gateway's Service). Open a support case for the install account early;
    the Service retries on its own once it's lifted.
  - The `small` profile costs roughly $250 a month while it runs (EKS, MSK, Aurora). Test
    installs set `deletion_protection = false` and are destroyed between uses.

## 2. Register the install with awzy

awzy staff register the install in the corp portal: client, environment, cloud, Region, and
release channel (`stable`, `fast`, or a pinned version). Registration gives you, once:

| Credential | Used for |
| --- | --- |
| Install ID (`install_<uuid>`) | The registry user name, and the install's identity in heartbeats |
| Pull token | Pulling signed images and the chart from `registry.awzysecurity.com` |
| Reporting token | The heartbeat awzy-reporter sends |

Keep all three in your own secret store. The example client keeps them as JSON
(`install_id`, `pull_token`, `reporting_token`) in its management account's Secrets Manager as
`awzy/install-credentials`, and its bootstrap reads them from there.

## 3. Configure your Terraform

Your own root modules hold the provider and backend settings; awzy's modules hold none. The
example client's are a template:

```sh
cd deploy/terraform/examples/example-client
cp deployment.env.example deployment.env     # gitignored; fill in accounts, Region, bucket, hostname
source deployment.env
```

Its settings are `TF_VAR_*` variables plus `TF_CLI_ARGS_init` for the state backend.
Back the file up, since it's the only record of what the deployment targets:
`tools/deployment-env push example-client`.

## 4. Apply the cloud layer

This step is per cloud; AWS's is below. On a cluster you already have, skip to step 5.

```sh
cd install
tofu init && tofu plan -out=install.plan     # read the plan
tofu apply install.plan                      # 20–30 minutes, most of it EKS and MSK
```

`aws/install` takes `azs` (two zones), `registry_prefix` (`registry.awzysecurity.com/awzy`),
and optionally `size_profile`, `vpc_cidr`, `eks_version`, `eks_public_access_cidrs`, and
`deletion_protection`. Its one output, `install_contract`, is everything bootstrap needs.

`size_profile` sets what costs the most:

| Profile | NAT gateways | Aurora | Kafka (MSK) |
| --- | --- | --- | --- |
| `small` (default) | One, shared by both zones | Scales to zero after 10 idle minutes | Two `kafka.t3.small` brokers on Kafka 3.9 |
| `standard`, `large` | One per zone | Never below 0.5 capacity units | Two `kafka.m7g.large` brokers on Kafka 4.1 (KRaft) |

Only `small` has been sized and measured so far.

**Check:** `aws eks update-kubeconfig --name awzy --region <region>` and
`kubectl get nodes` lists ready nodes.

## 5. Apply the cluster layer

Bootstrap takes:

| Input | From |
| --- | --- |
| `install_contract` | The cloud layer's output (the example reads it from remote state) |
| `chart_version` | Leave it unset to follow the channel you registered with: awzy's registry shows your install only the version that channel (or your pinned version) names. Set an exact version to hold the install there |
| `registry_credentials` | Install ID and pull token |
| `reporter` | Install ID and reporting token, or `null` to send no heartbeat |
| `cosign_public_key` | [`deploy/release/cosign.pub`](deploy/release/cosign.pub); Kyverno rejects images not signed with it |
| `support_public_key` | awzy's support key (`deploy/release/support.pub`), so awzy staff can sign in to a support session your admin approves; `null` leaves support sessions unable to sign in |
| `ingress` | The portal hostname and the Let's Encrypt contact |
| `client_portal` | Your IdP's issuer, JWKS URL, and the portal's public URL; the token audiences for the portal (`audience`, default `awzy`) and for its MCP endpoint (`mcp_audience`, default `awzy-mcp`); and `subject_claim`, the claim holding a person's SCIM user name when `sub` doesn't (default `sub`) |
| `awzy_bot` | Whether Awzy Bot runs; needs Claude enabled in your cloud's model service (Bedrock on AWS) |
| `investigations`, `investigation_budget` | Whether awzy-reasoner investigates the people it flags, and its monthly token budget and privileged groups; needs Claude in your cloud's model service |
| `llm_model` | The model ID for both, when the default isn't reachable directly |
| `slack` | Your Slack app's bot token and the security team's channel, for notifications; `null` for none |
| `mcp_gateway` | awzy-gateway, the MCP proxy your agents call: its hostname (with a DNS record to the load balancer; null keeps it in-cluster), its token audience (`audience`, default `awzy-gateway`), the MCP servers it fronts, and the tiers that may fail open while the PDP is unreachable; `null` for none |
| `intake` | awzy-intake, for your apps' and audit logs' events: its hostname for sources outside the cluster (null keeps it in-cluster); `null` for none |
| `drive` | The Google Drive connector: the member user it acts as (a Content manager of each shared drive to cover), an admin to name group shares, and the Google service account's JSON key, with domain-wide delegation for `drive.metadata.readonly` and `admin.directory.group.readonly`; `null` for none |
| `jira` | The Jira connector: your site's URL, an account that can browse the projects, the projects (default: all), and its API token; `null` for none |
| `clickhouse_retention_days` | How many days ClickHouse keeps decisions and events; `0` (the default) keeps them all. See the docs site's "Data retention and backups" |
| `break_glass_public_keys` | The PEM public keys of the admins who may roll back a policy bundle that locks everyone out, without the portal or your IdP; `null` turns break-glass off |
| `demo_idp`, `demo_data`, `demo_activity` | **Demo installs only:** the dev IdP, the synthetic company, and its everyday tool calls through awzy-gateway each weekday (needs `mcp_gateway`) |

On a cluster you already have, rather than one the cloud layer made, check it first from a
checkout of this repository. `contract.json` is your `install_contract` as JSON. The check only
reads, and says what to fix:

```sh
deploy/terraform/bootstrap/preflight.sh --context <your context> --contract contract.json
```

```sh
cd ../bootstrap
tofu init && tofu plan -out=bootstrap.plan
tofu apply bootstrap.plan
```

It makes these Secrets, none of which leave the cluster:

| Secret | Namespaces | For |
| --- | --- | --- |
| `awzy-clickhouse` | data, portal | ClickHouse's admin user |
| `awzy-clickhouse-reporter` | data, system | awzy-reporter's read-only user: disk sizes and Kafka positions |
| `awzy-clickhouse-reasoner` | data, system | awzy-reasoner's read-only user: decisions and events |
| `awzy-reporting-token` | system | The heartbeat, when `reporter` is set |
| `awzy-scim-token` | system | Your IdP's SCIM provisioning |
| `awzy-policy-verify` | system | The public half of the cloud key awzy-policy signs policy bundles, enforcement mode changes, and the outage fallback with, which the PDPs and awzy-gateway check them against. The private half never leaves your cloud's key service (KMS on AWS), and only awzy-policy's workload identity may sign with it |
| `awzy-policy-break-glass` | system | The break-glass admins' public keys, when `break_glass_public_keys` is set |
| `awzy-intake-token` | system | Event sources, when `intake` is set; read it from the `intake` output |
| `awzy-connectors-drive`, `awzy-connectors-jira` | system | The connectors' credentials, when `drive` or `jira` is set |
| `awzy-registry`, `awzy-chart-repo` | system, data, portal, Kyverno, ops | Pulling images and the chart |

Postgres credentials stay in your cloud's secret store, and External Secrets reads them with its
workload identity. On AWS, that's Secrets Manager, where RDS keeps them, read with Pod Identity.

Argo CD then installs the chart into `awzy-system`, `awzy-portal`, and `awzy-data`, and keeps
the cluster on it.

## 6. DNS and your IdP

1. **DNS:** point the portal hostname (a CNAME) at the Gateway's load balancer:
   `kubectl -n awzy-ops get gateway awzy` shows its address. On Cloudflare, make it DNS only
   (not proxied): the Gateway terminates TLS itself. cert-manager gets the certificate once the
   name resolves, from inside the cluster too. Resolvers that looked the name up before the
   record existed cache the miss for the zone's negative TTL (30 minutes on Cloudflare), so
   issuing can wait that long; it retries on its own.
2. **SCIM:** `tofu output -json scim` gives the SCIM URL (`https://<hostname>/scim/v2`) and
   bearer token. Enter both in your IdP's provisioning settings (Okta, Entra) so people and
   groups reach awzy-context.
3. **Sign-in:** register the client portal as an OIDC application in your IdP, with the issuer
   and JWKS URL you gave bootstrap as `client_portal`. awzy takes tokens for three audiences, kept
   apart so a token handed to an agent for one doesn't open another: the portal's own (`awzy`),
   its MCP endpoint's (`awzy-mcp`), and awzy-gateway's (`awzy-gateway`). Set each in bootstrap
   if your IdP names them differently (Entra's audiences are app ID URIs, `api://…`). Every token
   has to carry the person's SCIM user name in `sub`, or in the claim you name as
   `client_portal.subject_claim` (usually `email` with Entra, whose `sub` is opaque). Someone whose
   claim doesn't match is checked with the groups in their token alone.

## 7. Check the install

- **Argo CD:** `kubectl -n awzy-ops get application awzy` is `Synced` and `Healthy`.
- **Pods:** `kubectl get pods -n awzy-system -n awzy-portal -n awzy-data` are all ready.
- **Images:** `kubectl get clusterpolicy awzy-verify-images` is ready. In the awzy namespaces
  it refuses any image that isn't from `registry_prefix` with the release key's signature,
  including third-party ones, which awzy mirrors and signs:
  `kubectl -n awzy-system run unsigned --image=busybox` is refused (`only-the-release-registry`).
  An image from awzy's registry without the signature is refused too
  (`signed-by-awzy-release-key: … no signatures found`); the example install checked that with a
  test tag pushed unsigned, then deleted.
- **A decision:** port-forward the PDP and ask it something (the request shape is in
  [DEVELOPMENT.md](DEVELOPMENT.md#pdp)). The PDP takes only callers with a Kubernetes
  service-account token for its audience, so make one for the backend's account:
  ```sh
  kubectl -n awzy-system port-forward svc/awzy-pdp 8080:8080 &
  token=$(kubectl -n awzy-system create token awzy --audience awzy-pdp --duration 10m)
  curl -s localhost:8080/v1/access/evaluation -H "authorization: Bearer $token" \
    -H 'content-type: application/json' -d '{…}'
  ```
  The answer has a `decision_…` QID, a verdict, and `"mode":"monitor"`. The verdict can differ
  from DEVELOPMENT.md's example: once your directory is in awzy-context, the PDP uses what it
  knows about a principal over the request's inline entities. Every install starts in `monitor`
  mode: decisions are made and recorded, and nothing is blocked until an admin switches a scope
  to enforce.
- **Heartbeat:** the install shows as reporting in the corp portal within a minute, and
  `kubectl -n awzy-system logs deploy/awzy-reporter` shows `sending heartbeat` each minute with
  no `heartbeat not delivered`.
- **The portal:** `https://<hostname>` signs you in through your IdP.

## 8. Monitoring (optional)

Every awzy service serves Prometheus metrics. On AWS, two more modules watch them for you:

- [`deploy/terraform/aws/monitoring`](deploy/terraform/aws/monitoring): Amazon Managed Service for
  Prometheus, fed by its managed scraper (nothing more runs in your cluster), with awzy's alert
  rules routed to an SNS topic and the `alert_emails` you give it; and an Amazon Managed Grafana
  workspace, signed in to through your IAM Identity Center, that reads Prometheus and CloudWatch
  (Aurora, MSK, ElastiCache). Pass it the cloud layer's `monitoring` output.
- [`deploy/terraform/grafana`](deploy/terraform/grafana): awzy's dashboards and data sources, in
  that workspace or any Grafana of yours. Apply it after the first, with the `grafana` output.

You can use your own monitoring instead: scrape each awzy pod's `/metrics` (port `http`), and load
[`deploy/observability`](deploy/observability)'s alert rules and dashboards into it.

## Operating an install

- **Upgrades** follow the release channel the install was registered with: Argo CD moves to the
  version the channel names within minutes of awzy moving it. `fast` gets every release as it's
  published; a release reaches `stable` after it has run on `fast` for at least a day and awzy
  promotes it. A pinned install stays on its version. awzy's registry serves an install only the
  version its channel names or the one it's pinned to, so holding an install on a version, or
  rolling it back to an earlier one, is a pin: ask awzy to pin the install in the corp portal, then
  set `chart_version` to that version and apply bootstrap. A `chart_version` the registry isn't
  serving shows in Argo CD as `ComparisonError … not found`. Apply bootstrap from the same awzy
  commit as the chart: a release can change what bootstrap makes for it (the policy signing key
  moved to the cloud's key service this way).
- **A release that needs a newer bootstrap waits.** Each release names the bootstrap level it needs,
  and bootstrap passes the install's. When a channel brings a release your bootstrap is too old for,
  Argo CD installs nothing and keeps the release you have running; `kubectl -n awzy-ops get
  application awzy` shows `ComparisonError` and a message naming the awzy commit to apply. Apply the
  cloud layer and then bootstrap from that commit, and the release installs. Upgrades and
  rollbacks were tested on the example install under load (the deployment plan's step 4). If a sync failed (a hook
  Job that kept failing, say), Argo CD keeps retrying that revision after you move on; stop it
  with `kubectl -n awzy-ops patch application awzy --type merge -p
  '{"status":{"operationState":{"phase":"Terminating"}}}'` and the next sync takes the new one.
- **Rotating credentials:** a new pull or reporting token comes from re-registering in the corp
  portal; update your secret store and apply bootstrap. Bootstrap's own passwords (ClickHouse,
  SCIM) rotate when you `tofu apply -replace` their `random_password`. Give your IdP the new SCIM
  token afterwards.
- **No heartbeat:** set `reporter = null`. Nothing then leaves the install.
- **Uninstalling:** destroy bootstrap, then the cloud layer. With `deletion_protection = true`
  (the default), the databases survive until you turn it off and apply.
