# Performance runs

One file per run of the deployment plan's scale scenarios
([deploy plan, "Scale scenarios"](../plans/deployment-plan.md)), named
`<date>-<install>.md`: what ran (profile, nodes, releases), how (load generator, clients,
duration), the numbers, and which pass conditions held. The platform plan's targets cite these.

Until the k6 harness exists (#144), runs use `pdp_loadtest`, which ships in the PDP image: gRPC
checks from a pod in the cluster, a third of them delegated (two Cedar evaluations), entities sent
inline. Its latencies are client-side, but over one gRPC connection, so they measure one PDP pod.
