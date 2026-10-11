# Performance runs

One file per run of the deployment plan's scale scenarios
([deploy plan, "Scale scenarios"](../plans/deployment-plan.md)), named
`<date>-<install>.md`: what ran (profile, nodes, releases), how (load generator, clients,
duration), the numbers, and which pass conditions held. The platform plan's targets cite these.

Until the k6 harness exists (#144), runs use `pdp_loadtest`, which ships in the PDP image: gRPC
checks from a pod in the cluster, a third of them delegated (two Cedar evaluations), entities sent
inline. Its latencies are client-side. Each client has its own gRPC connection, which the Service
places on one PDP pod, and the PDPs close connections after about a minute so clients spread over
pods as they're added. Runs before 2026-10-12 used one connection, so they measure one PDP pod.
