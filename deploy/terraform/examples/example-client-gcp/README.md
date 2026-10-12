# examples/example-client-gcp

The example client's install on Google Cloud: the same awzy, through `deploy/terraform/gcp/install`
and the cloud-agnostic `deploy/terraform/bootstrap`, at `gcp.demo.awzysecurity.com` (ADR 0106).
It stands in for a client's own infrastructure code, as `examples/example-client` does on AWS, and
never references awzy's accounts or state.

| Module | What |
| --- | --- |
| `install/` | the cloud layer (`gcp/install`) in the install's project |
| `bootstrap/` | the cluster layer |

What it leaves out on Google Cloud so far: ClickHouse backups (GCS's S3 API needs HMAC keys),
Awzy Bot and investigations (Vertex AI), connector credentials from the client portal (it writes
to AWS Secrets Manager only), and managed monitoring.

## Once per project

1. Copy `deployment.env.example` to `deployment.env` (gitignored), fill it in, and `source` it.
   Back it up in the project's own Secret Manager (`tools/deployment-env push
   example-client-gcp`); restore it with `tools/deployment-env pull example-client-gcp --project
   $TF_VAR_project`.
2. Make the state bucket: `gcloud storage buckets create gs://$TF_VAR_state_bucket --project
   $TF_VAR_project --location $TF_VAR_region --uniform-bucket-level-access`, and turn on its
   versioning (`gcloud storage buckets update gs://$TF_VAR_state_bucket --versioning`).
3. Register the install in the corp portal (cloud GCP), and keep what it gives in Secret Manager:
   `printf '{"install_id":"…","pull_token":"…","reporting_token":"…"}' | gcloud secrets create
   awzy-install-credentials --project $TF_VAR_project --data-file=-`.

## Apply

```sh
cd install && tofu init && tofu apply      # the cluster, Cloud SQL, KMS, and Kafka
cd ../bootstrap && tofu init && tofu apply # Argo CD and the rest, then the awzy chart
```

Then point the demo hostname (a DNS-only CNAME) at the Gateway's address:
`kubectl -n awzy-ops get gateway awzy`.

## Applied by CI

After the first apply, CI plans and applies `install/` and `bootstrap/` (ADR 0115,
`.github/workflows/terraform.yml`): a pull request that changes them gets a comment listing what
merging would change, and after merge the apply waits in the `example-gcp` GitHub environment for
a person's approval.

The example client grants CI that in its own project, as a client letting a vendor's pipeline
manage a project would: `install/ci.tf` trusts GitHub's OIDC tokens through workload identity
federation, for two service accounts. The plan identity, for pull requests and main, reads the
project, the state, and the install's credentials. The apply identity, an owner, is only for the
`example-gcp` environment.

### Setting it up (once)

The identities are applied by hand, since a pipeline can't create the identity it runs as:

1. Add `TF_VAR_github_oidc_subject_repo` to `deployment.env` (see `deployment.env.example`).
2. Apply `install/` by hand as before.
3. Create the environment with a required reviewer, and set the identities and inputs:

```sh
gh api -X PUT repos/{owner}/{repo}/environments/example-gcp \
  -F 'reviewers[][type]=User' -F "reviewers[][id]=$(gh api user --jq .id)" \
  -F 'deployment_branch_policy[protected_branches]=true' -F 'deployment_branch_policy[custom_branch_policies]=false'
gh variable set AWZY_CORP_EXAMPLE_GCP_WIF_PROVIDER --body "$(cd install && tofu output -json ci | jq -r .provider)"
gh variable set AWZY_CORP_EXAMPLE_GCP_PLAN_SERVICE_ACCOUNT --body "$(cd install && tofu output -json ci | jq -r .service_accounts.plan)"
gh variable set AWZY_CORP_EXAMPLE_GCP_TERRAFORM_SERVICE_ACCOUNT --body "$(cd install && tofu output -json ci | jq -r .service_accounts.terraform)"
gh secret set AWZY_CORP_EXAMPLE_GCP_DEPLOYMENT_ENV < deployment.env
```

After that, change `deployment.env` by updating the secret too; CI reads only the secret.
