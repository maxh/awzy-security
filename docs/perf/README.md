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

`pdp_loadtest <endpoint> <clients> <seconds>` is closed loop: each client sends its next check when
the last is answered, so a slow PDP is sent less, and its latencies understate the tail. Those runs
measure capacity, not the latency budget. `pdp_loadtest <endpoint> <connections> <seconds> <rate>`
is open loop: checks fall due at `rate` per second however the PDP is doing, each is timed from
when it was due, and a line a minute shows latency over the run. The pass conditions ("p99 under
10 ms at 1,500 checks/s") are open-loop numbers (#350).
