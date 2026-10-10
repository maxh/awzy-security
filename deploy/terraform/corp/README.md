# deploy/terraform/corp

The vendor side of the deployment (see `docs/deployment-plan.md`): what runs in the
organization that builds and ships awzy. Applied with OpenTofu (`tofu`); plain Terraform works
too.

| Module | Account | What |
| --- | --- | --- |
| `state/` | management | S3 bucket for this tree's state |
| `org/` | management | member accounts (`member_accounts`); the RCP |
| `build/` | build | ECR repositories, cosign key, CI publish role |
| `corp-portal/` | build, for now | `corp.<domain>` and `registry.<domain>` (ADR 0028): ECS Fargate, load balancer, Aurora, Cloudflare DNS |

## Configuration

Nothing here names a particular deployment. Each module reads its accounts, region, state
bucket, domain, and the GitHub repository allowed to publish from variables
(`variables.tf`, shared by every module). Copy `deployment.env.example` to `deployment.env`
(gitignored), fill it in, and source it before running tofu:

```sh
cp deployment.env.example deployment.env   # then edit
source deployment.env
cd build && tofu init && tofu plan
```

`deployment.env` also passes the state bucket to `tofu init` (`TF_CLI_ARGS_init`), since a
backend block can't read variables. Keep a copy somewhere safe: it's the only record of which
accounts this tree manages.

CI publishes with the repository's Actions variables, set from `build/`'s outputs:
`AWZY_CORP_AWS_DEPLOY_ROLE_ARN` (from `corp-portal/`), `AWZY_CORP_AWS_PUBLISH_ROLE_ARN`,
`AWZY_CORP_AWS_REGION`, `AWZY_CORP_COSIGN_KEY`, and `AWZY_CORP_RELEASE_REGISTRY`.
Without them, CI tests and skips publishing.

## Credentials

Modules use the AWS CLI profiles named in `deployment.env` (`TF_VAR_management_profile`,
`TF_VAR_build_profile`); sign in to each first.

For an organization AWS manages (awzy's is one: root sign-in is disabled and every account
shares one root email), console access goes through the IAM Identity Center access portal's
**Applications → AWS account access**, which lists the accounts and their
`AccountFullAccessRole`. Open the account's console there, then `aws login --profile <name>
--region <region>` and pick that account's session; check the account ID before overwriting a
profile. Each provider sets `allowed_account_ids`, so a run against the wrong account fails before it
changes anything.

## First run (already done)

`state/` creates the bucket its own backend points at, so it was applied once without the
backend and then migrated:

```sh
cd state
echo 'terraform {
  backend "local" {}
}' > backend_override.tf                    # temporarily use local state
tofu init && tofu apply                     # creates the bucket
rm backend_override.tf
tofu init -migrate-state                    # moves the state into the bucket
rm terraform.tfstate*
```

After that, every module is plain `tofu init && tofu plan` (with `deployment.env` sourced).

Modules keep their state in the management account and work in their own account, so the
backend uses the management profile and each provider its own. `corp-portal/` also reads a
Cloudflare API token from `TF_VAR_cloudflare_token_file` (default `~/.config/awzy/cf-token`).

## Corp portal accounts

Staff accounts (ADR 0025) live in the `awzy-corp-portal/accounts` secret as the JSON list
`awzy-corp-user` prints, never in Terraform state:

```sh
aws secretsmanager put-secret-value --profile <build profile> \
  --secret-id awzy-corp-portal/accounts --secret-string file://accounts.json
aws ecs update-service --profile <build profile> \
  --cluster awzy-corp-portal --service awzy-corp-portal --force-new-deployment
```

## Fleet alerts

Alerts for installs that become degraded, stop reporting, or recover (ADR 0043) go to a Slack
incoming webhook. Create one for the channel in Slack (an app with Incoming Webhooks turned on),
then store its URL, which Terraform never sees:

```sh
aws secretsmanager put-secret-value --profile <build profile> \
  --secret-id awzy-corp-portal/alerts-slack-webhook --secret-string 'https://hooks.slack.com/services/…'
aws ecs update-service --profile <build profile> \
  --cluster awzy-corp-portal --service awzy-corp-portal --force-new-deployment
```

Until it's set, the corp portal logs each transition and sends nothing.

Deploying a new version: `tofu apply -var image_tag=<sha> -var registry_image_tag=<sha>` with a
commit CI has published.

## Registry pull tokens

Until the corp portal issues them at registration (ADR 0028), staff make an install's pull token
with the registry binary, run as a one-off task (it prints the token once):

```sh
awzy-registry token create <install_id> <your username>
awzy-registry token revoke <install_id>
```

## Not in Terraform

Made by hand during setup and deliberately left out:

- Cloudflare Email Routing (rules forwarding the accounts' root emails) and DNSSEC
- the AWS Budgets `awzy-org-monthly` and `example-client-org-monthly`
- the example client's Identity Center instance (console only), user `max`, permission set
  `AdministratorAccess`, and its account assignments
- what AWS created on activation: the organization, Identity Center, the delegated admin
  account, and the AWS-managed SCPs (only the RCP is managed, in `org/`)
