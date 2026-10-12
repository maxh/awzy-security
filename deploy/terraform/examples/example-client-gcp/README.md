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
