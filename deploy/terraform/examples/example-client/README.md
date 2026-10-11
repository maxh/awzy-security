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
