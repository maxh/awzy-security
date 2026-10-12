# examples/example-client

Stands in for a client's own infrastructure code (see `docs/plans/deployment-plan.md`). Runs in the
example client's own organization and never references the vendor's accounts or state.

| Module | What |
| --- | --- |
| `state/` | S3 state bucket in the example client's management account |
| `org/` | member account `awzy-example-install` |
| `install/` | the cloud layer (`aws/install`) in the install account |
| `monitoring/` | `aws/monitoring`: managed Prometheus, alerts, the trace collector, and Grafana |
| `bootstrap/` | the cluster layer; sends traces to `monitoring/`'s collector once it's applied |
| `grafana/` | awzy's data sources and dashboards in that Grafana |

Configuration, credentials, and the one-time `state/` bootstrap work as in
`deploy/terraform/corp/README.md`: copy `deployment.env.example` here to `deployment.env`, fill
in the example client's accounts, region, state bucket, registry, and hostname, and source it. Its backup lives in the example client's own management account
(`awzy/deployment-env/example-client`; `tools/deployment-env push example-client`), never
the vendor's.

## Applying

`install/`, `monitoring/`, `grafana/`, and `bootstrap/` are applied by CI (ADR 0115), in the
`terraform` workflow's `example-aws` deployment, in that order:

- **On a pull request** that changes them, or the `aws/` and `bootstrap/` modules they use, it
  plans each against its recorded state and keeps a comment on the pull request listing what
  merging would change.
- **After merge**, the apply waits in the GitHub environment `example-aws` until someone approves
  it, then refreshes, plans, and applies each.

The example client grants CI that in its own accounts, as a client letting a vendor's pipeline
manage an account would: `install/ci.tf` makes the install account's roles (read-only for plans,
admin for applies), and `state/ci.tf` the management account's, which reach only the state and
the install credentials. Only the `example-aws` environment can assume the apply roles. The
cluster's admins are listed (`cluster_admins`), so access doesn't move to whoever applied last.
`state/` and `org/` stay with a person.

### Setting it up (once)

The roles are applied by hand, since a pipeline can't create the role it runs as:

1. Add `TF_VAR_github_oidc_subject_repo` and `TF_VAR_cluster_admins` to `deployment.env` (see
   `deployment.env.example`). `operator` is the cluster's creator, as Terraform recorded it:
   `tofu state show 'module.install.module.eks.aws_eks_access_entry.this["cluster_creator"]'` in
   `install/` shows its `principal_arn`.
2. Apply `state/`, then `install/` (which also moves the creator's access to `operator`), then
   `bootstrap/`, by hand as before.
3. Create the environment with a required reviewer, and set the roles and inputs:

```sh
gh api -X PUT repos/{owner}/{repo}/environments/example-aws \
  -F 'reviewers[][type]=User' -F "reviewers[][id]=$(gh api user --jq .id)" \
  -F 'deployment_branch_policy[protected_branches]=true' -F 'deployment_branch_policy[custom_branch_policies]=false'
gh variable set AWZY_CORP_EXAMPLE_AWS_PLAN_ROLE_ARN --body "$(cd install && tofu output -json ci_role_arns | jq -r .plan)"
gh variable set AWZY_CORP_EXAMPLE_AWS_TERRAFORM_ROLE_ARN --body "$(cd install && tofu output -json ci_role_arns | jq -r .terraform)"
gh variable set AWZY_CORP_EXAMPLE_AWS_STATE_PLAN_ROLE_ARN --body "$(cd state && tofu output -json ci_role_arns | jq -r .plan)"
gh variable set AWZY_CORP_EXAMPLE_AWS_STATE_TERRAFORM_ROLE_ARN --body "$(cd state && tofu output -json ci_role_arns | jq -r .terraform)"
gh secret set AWZY_CORP_EXAMPLE_AWS_DEPLOYMENT_ENV < deployment.env
```

After that, change `deployment.env` by updating the secret too; CI reads only the secret.
