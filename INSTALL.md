# Installing awzy

How to install awzy in your own cloud account. Developing awzy is in
[DEVELOPMENT.md](DEVELOPMENT.md); why an install looks the way it does is in
[docs/deployment-plan.md](docs/deployment-plan.md).

> **Untested as written.** This runbook was written from the Terraform and the chart before the
> first full install. It will be corrected during the example client's first install
> ([deploy/terraform/examples/example-client](deploy/terraform/examples/example-client/README.md)),
> which is the worked example throughout.

An install runs entirely in your account. awzy (the vendor) holds no credential for it: the
install pulls signed releases from awzy's registry and sends a heartbeat out, and nothing comes
in. AWS is the only cloud so far.

It has two layers, applied in order:

| Layer | Module | What it makes |
| --- | --- | --- |
| Cloud | [`deploy/terraform/aws/install`](deploy/terraform/aws/install) | VPC, EKS, Aurora Postgres, MSK (Kafka), ElastiCache (Valkey), an S3 bucket, a KMS key, and Pod Identity roles. Outputs `install_contract`. |
| Cluster | [`deploy/terraform/bootstrap`](deploy/terraform/bootstrap) | Namespaces, Secrets, Argo CD, Kyverno, External Secrets, the ClickHouse operator, Envoy Gateway, and cert-manager, then an Argo CD Application for the awzy chart. Never cloud-specific. |

You call both from your own Terraform, as
[`examples/example-client`](deploy/terraform/examples/example-client) does.

## 1. Before you start

- **Tools:** OpenTofu ≥ 1.10 (or Terraform), the AWS CLI, and kubectl.
- **An AWS account for the install**, separate from the one that holds Terraform state. The
  example client keeps state in its management account and installs into a member account
  (`awzy-example-install`).
- **Region and SCPs:** pick one Region with two availability zones. The install works under
  SCPs that deny other Regions and deny IAM users with access keys; it uses neither.
- **A hostname** for the client portal (`awzy.example.com`) in DNS you control, and an email
  address for Let's Encrypt.
- **Cost:** the `small` profile costs roughly $250 a month while it runs (EKS, MSK, Aurora). Test
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
| `chart_version` | The release to install, or what your channel names |
| `registry_credentials` | Install ID and pull token |
| `reporter` | Install ID and reporting token, or `null` to send no heartbeat |
| `cosign_public_key` | [`deploy/release/cosign.pub`](deploy/release/cosign.pub); Kyverno rejects images not signed with it |
| `ingress` | The portal hostname and the Let's Encrypt contact |
| `client_portal` | Your IdP's issuer, JWKS URL, and the portal's public URL |
| `awzy_bot` | Whether Awzy Bot runs; needs Claude enabled in the install account's Bedrock |
| `investigations`, `investigation_budget` | Whether awzy-reasoner investigates the people it flags, and its monthly token budget and privileged groups; needs Claude in Bedrock |
| `llm_model` | The Bedrock model ID for both, when the default isn't reachable directly |
| `slack` | Your Slack app's bot token and the security team's channel, for notifications; `null` for none |
| `mcp_gateway` | awzy-gateway, the MCP proxy your agents call: its hostname (with a DNS record to the load balancer; null keeps it in-cluster), the MCP servers it fronts, and the tiers that may fail open while the PDP is unreachable; `null` for none |
| `intake` | awzy-intake, for your apps' and audit logs' events: its hostname for sources outside the cluster (null keeps it in-cluster); `null` for none |
| `drive` | The Google Drive connector: the member user it acts as (a Content manager of each shared drive to cover), an admin to name group shares, and the Google service account's JSON key, with domain-wide delegation for `drive.metadata.readonly` and `admin.directory.group.readonly`; `null` for none |
| `jira` | The Jira connector: your site's URL, an account that can browse the projects, the projects (default: all), and its API token; `null` for none |
| `break_glass_public_keys` | The PEM public keys of the admins who may roll back a policy bundle that locks everyone out, without the portal or your IdP; `null` turns break-glass off |
| `demo_idp`, `demo_data` | **Demo installs only:** the dev IdP and the synthetic company |

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
| `awzy-policy-signing`, `awzy-policy-verify` | system | The key awzy-policy signs policy bundles and enforcement mode changes with, and its public half, which the PDPs check them against |
| `awzy-policy-break-glass` | system | The break-glass admins' public keys, when `break_glass_public_keys` is set |
| `awzy-intake-token` | system | Event sources, when `intake` is set; read it from the `intake` output |
| `awzy-connectors-drive`, `awzy-connectors-jira` | system | The connectors' credentials, when `drive` or `jira` is set |
| `awzy-registry`, `awzy-chart-repo` | system, data, portal, Kyverno, ops | Pulling images and the chart |

Postgres credentials stay in Secrets Manager, where RDS keeps them; External Secrets reads them
with Pod Identity.

Argo CD then installs the chart into `awzy-system`, `awzy-portal`, and `awzy-data`, and keeps
the cluster on it.

## 6. DNS and your IdP

1. **DNS:** point the portal hostname (a CNAME) at the Gateway's load balancer:
   `kubectl -n awzy-ops get gateway awzy` shows its address. cert-manager gets the certificate
   once the name resolves.
2. **SCIM:** `tofu output -json scim` gives the SCIM URL (`https://<hostname>/scim/v2`) and
   bearer token. Enter both in your IdP's provisioning settings (Okta, Entra) so people and
   groups reach awzy-context.
3. **Sign-in:** register the client portal as an OIDC application in your IdP, with the issuer
   and JWKS URL you gave bootstrap as `client_portal`. Its access tokens are for the audience
   `awzy`, and carry each person's SCIM user name as `sub`: awzy matches people by `sub`, and an
   install can't choose another claim yet. Someone whose `sub` doesn't match is checked with the
   groups in their token alone.

## 7. Check the install

- **Argo CD:** `kubectl -n awzy-ops get application awzy` is `Synced` and `Healthy`.
- **Pods:** `kubectl get pods -n awzy-system -n awzy-portal -n awzy-data` are all ready.
- **Images:** `kubectl get clusterpolicy awzy-verify-images` is ready. In the awzy namespaces
  it refuses any image that isn't from `registry_prefix` with the release key's signature,
  including third-party ones, which awzy mirrors and signs:
  `kubectl -n awzy-system run unsigned --image=busybox` is refused.
- **A decision:** port-forward the PDP and ask it something (the request shape is in
  [DEVELOPMENT.md](DEVELOPMENT.md#pdp)):
  ```sh
  kubectl -n awzy-system port-forward svc/awzy-pdp 8080:8080
  curl -s localhost:8080/v1/access/evaluation -H 'content-type: application/json' -d '{…}'
  ```
  Every install starts in `monitor` mode: decisions are made and recorded, and nothing is
  blocked until an admin switches a scope to enforce.
- **Heartbeat:** the install shows as reporting in the corp portal within a minute.
- **The portal:** `https://<hostname>` signs you in through your IdP.

## Operating an install

- **Upgrades** follow the release channel the install was registered with. A pinned install
  upgrades when you change `chart_version` and apply bootstrap again.
- **Rotating credentials:** a new pull or reporting token comes from re-registering in the corp
  portal; update your secret store and apply bootstrap. Bootstrap's own passwords (ClickHouse,
  SCIM) rotate when you `tofu apply -replace` their `random_password`. Give your IdP the new SCIM
  token afterwards.
- **No heartbeat:** set `reporter = null`. Nothing then leaves the install.
- **Uninstalling:** destroy bootstrap, then the cloud layer. With `deletion_protection = true`
  (the default), the databases survive until you turn it off and apply.
