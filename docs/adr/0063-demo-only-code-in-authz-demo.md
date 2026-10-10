# 0063. Demo-only code lives in authz/demo, and nothing else may depend on it

- Status: Accepted
- Date: 2026-10-10
- Amends: [0040](0040-authz-grouped-into-services-jobs-and-libs.md) (a fourth folder in `authz/`)

## Context

0040 put the demo's synthetic company (0039) in `authz/jobs/synthetic`, since it was then a job:
it provisioned Larkspur Capital and exited. Since then it has also become an activity driver, a
history sender, and two long-running MCP servers that awzy-gateway fronts (larkspur-drive, 0050;
larkspur-admin, 0064). Neither `jobs/` nor `services/` describes that.

More important, nothing kept real code from depending on it. Its visibility was
`//tools:client_side`, so awzy-pdp or awzy-gateway could have imported it and Bazel would have
built them. Demo code ending up in a production service's dependencies is the kind of mistake the
build should refuse, as it already refuses the corp portal depending on client-side code (0009).

## Decision

- **`authz/demo/<name>` holds code only for demo installs and tests:** the synthetic company now,
  and whatever else stands in for a client's systems later. It stays under `authz/`, because it
  only exists to drive the client side and ships in the client chart behind a demo-only value.
- **Nothing outside `authz/demo` may depend on it.** It's visible only to `//tools:demo_only`
  (`//authz/demo/...`). Its tests may depend on anything client-side, since that's what they run.
- **It's still released like the other images.** CI publishes every image under `//authz/...`,
  and the chart runs it only when `demoData.enabled` is set, so a real install never starts it.
- **The synthetic company stays one crate and one binary.** Its MCP stand-ins share its generator,
  its document model, and the awzy-context client, and splitting them into separate binaries would
  mean more images for a few hundred lines. If one grows its own dependencies or deployment, it
  becomes its own crate in `authz/demo/`.

Rejected: a top-level `demo/` folder. 0011 keeps code with the area it serves, and this would
need its own copy of the client-side boundary.

## Consequences

- Labels change from `//authz/jobs/synthetic` to `//authz/demo/synthetic`. The crate, binary, and
  image names don't change.
- Code that wants the synthetic company's data in a test (a classifier evaluation, a load test)
  belongs in `authz/demo/` itself, or gets the data through awzy's own APIs.
