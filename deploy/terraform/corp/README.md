# deploy/terraform/corp

The vendor side of the deployment (see `docs/plans/deployment-plan.md`): what runs in the
organization that builds and ships awzy. Applied with OpenTofu (`tofu`, 1.8 or later): the
backends read variables, which plain Terraform can't.

Each module keeps its state in the account it works in, so routine changes in the build account
never need the management account.

| Module | Account (and its state) | What |
| --- | --- | --- |
| `state/` | management | the state buckets: one in the management account, one in the build account |
| `org/` | management | member accounts (`member_accounts`); the RCP |
| `build/` | build | ECR repositories, cosign key, CI publish role |
| `corp-portal/` | build, for now | `corp.<domain>`, `registry.<domain>` (ADR 0028), and `docs.<domain>`: ECS Fargate, load balancer, Aurora, S3 and CloudFront, Cloudflare DNS |

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

`deployment.env` names the state buckets too (`TF_VAR_state_bucket`, `TF_VAR_build_state_bucket`),
which the backends read. It's the only record of which accounts this tree manages, so a copy
lives in Secrets Manager in the management account (`awzy/deployment-env/corp`), and another on
this machine in `~/backup/awzy/deployment-env/` (`AWZY_DEPLOYMENT_ENV_DIR`), for when the
checkout is gone and the AWS session has expired:

```sh
tools/deployment-env push corp                                          # after editing it; copies locally too
tools/deployment-env pull corp --profile awzy-corp --region us-east-2   # on a new machine
tools/deployment-env pull corp --local                                  # from this machine's copy
```

CI publishes with the repository's Actions variables, set from `build/`'s outputs:
`AWZY_CORP_AWS_DEPLOY_ROLE_ARN` (from `corp-portal/`), `AWZY_CORP_AWS_PUBLISH_ROLE_ARN`,
`AWZY_CORP_AWS_REGION`, `AWZY_CORP_COSIGN_KEY`, and `AWZY_CORP_RELEASE_REGISTRY`.
Without them, CI tests and skips publishing.

## Credentials

Modules use the AWS CLI profiles named in `deployment.env`: `build/` and `corp-portal/` only
the build account's (`TF_VAR_build_profile`), `org/` only the management account's
(`TF_VAR_management_profile`), and `state/` both. Sign in to the ones you need first.

For an organization AWS manages (awzy's is one: root sign-in is disabled and every account
shares one root email), console access goes through the IAM Identity Center access portal's
**Applications → AWS account access**, which lists the accounts and their
`AccountFullAccessRole`. Open the account's console there, then `aws login --profile <name>
--region <region>` and pick that account's session; check the account ID before overwriting a
profile. Each provider sets `allowed_account_ids`, so a run against the wrong account fails before it
changes anything.

## Applying, and the drift check

`build/` and `corp-portal/` are applied by CI (ADR 0115), in the `terraform-corp` workflow:

- **On a pull request** that changes them, it plans both against their recorded state, as the
  read-only role `awzy-ci-plan` (`corp-portal/drift.tf`), and keeps a comment on the pull request
  listing what merging would change.
- **After merge**, the apply waits in the GitHub environment `corp-terraform` until someone
  approves it, then refreshes, plans, and applies both, as `awzy-ci-terraform`
  (`corp-portal/terraform-ci.tf`), which only that environment can assume.
- **Daily**, the `terraform-drift` workflow plans both again and keeps an issue open for each
  module with changes waiting, until they're applied: an apply nobody approved, or one that
  failed.

Logs and comments print resource addresses only, never plan details, since those hold account
IDs. `state/` and `org/` run in the management account and stay with a person, with the
credentials above.

CI needs, from `corp-portal/`'s outputs and files here:

| Where | Name | Value |
| --- | --- | --- |
| Repository variable | `AWZY_CORP_AWS_PLAN_ROLE_ARN` | `tofu output ci_plan_role_arn` |
| Repository secret | `AWZY_CORP_DEPLOYMENT_ENV` | this `deployment.env` |
| Repository variable | `AWZY_CORP_AWS_TERRAFORM_ROLE_ARN` | `tofu output ci_terraform_role_arn` |
| `corp-terraform` environment secret | `AWZY_CORP_CLOUDFLARE_TOKEN` | the Cloudflare API token |

The two roles are the one part applied by hand, once, since a pipeline can't create the role it
runs as. Then create the environment with a required reviewer and set the rest:

```sh
gh api -X PUT repos/{owner}/{repo}/environments/corp-terraform \
  -F 'reviewers[][type]=User' -F "reviewers[][id]=$(gh api user --jq .id)" \
  -F 'deployment_branch_policy[protected_branches]=true' -F 'deployment_branch_policy[custom_branch_policies]=false'
gh variable set AWZY_CORP_AWS_TERRAFORM_ROLE_ARN --body "$(cd corp-portal && tofu output -raw ci_terraform_role_arn)"
gh secret set AWZY_CORP_CLOUDFLARE_TOKEN --env corp-terraform < ~/.config/awzy/cf-token
```

## First run (already done)

`state/` creates the bucket its own backend points at, so it was applied once without the
backend and then migrated. Its second bucket, in the build account, came later; `build/` and
`corp-portal/` moved their state there with `tofu init -migrate-state`.

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
`corp-portal/` also reads a Cloudflare API token from `TF_VAR_cloudflare_token_file` (default
`~/.config/awzy/cf-token`).

## Corp portal accounts

Staff accounts (ADR 0025) live in the `awzy-corp-portal/accounts` secret as the JSON list
`awzy-corp-user` prints, never in Terraform state:

```sh
aws secretsmanager put-secret-value --profile <build profile> \
  --secret-id awzy-corp-portal/accounts --secret-string file://accounts.json
aws ecs update-service --profile <build profile> \
  --cluster awzy-corp-portal --service awzy-corp-portal --force-new-deployment
```

## Support sessions

Staff sign in to a client's portal during a session the client approved with a token the corp
portal signs (ADR 0095). The private key lives only in the `awzy-corp-portal/support-signing-key`
secret; its public half is `deploy/release/support.pub`, which installs trust. Make the pair once:

```sh
openssl ecparam -name prime256v1 -genkey -noout | openssl pkcs8 -topk8 -nocrypt > support.pem
openssl ec -in support.pem -pubout > deploy/release/support.pub        # commit this
aws secretsmanager put-secret-value --profile <build profile> \
  --secret-id awzy-corp-portal/support-signing-key --secret-string file://support.pem
rm support.pem
aws ecs update-service --profile <build profile> \
  --cluster awzy-corp-portal --service awzy-corp-portal --force-new-deployment
```

Until it's set, staff can ask for sessions but can't sign in to them.

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
