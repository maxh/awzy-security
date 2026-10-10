---
name: local-stack
description: Run awzy locally against real Postgres, ClickHouse, Kafka, and Valkey, run the integration tests that otherwise skip, and bring services up to try a change end to end. Use for database, Kafka, or ClickHouse work, before claiming such code works, or when asked to run the app.
---

# Local stack

Integration tests read `AWZY_TEST_*` variables and **skip, passing, when they're unset**. A
green `bazel test //...` says nothing about database code until the local stack has run.

## Start it

```sh
df -h .                                            # Docker images and Bazel need several GB
docker compose -f deploy/local/compose.yaml up -d --wait
docker compose -f deploy/local/compose.yaml ps     # all healthy
```

Postgres `localhost:5432` (user and password `awzy`), ClickHouse `localhost:8123`, Kafka
`localhost:9092` (`kafka:29092` from inside Docker, which is what ClickHouse uses), Valkey
`localhost:6379`.

## Run the integration tests

```sh
bazel test --config=local-stack //...                       # everything
bazel test --config=local-stack //authz/services/policy:policy_test  # one crate
```

To confirm a test really ran rather than skipped, look for `skipping:` in its log
(`bazel-testlogs/<package>/<target>/test.log`).

## Run services

DEVELOPMENT.md has the exact commands and environment for each, and a table of every service's local
port ("Running the services"): awzy-policy (`:8081`),
the ClickHouse migrations, awzy-pdp (`:8082`, logging decisions through Kafka), the dev IdP
(`https://127.0.0.1:5556`), and the client portal (`:8080`). Start them in that order, each in
the background, and check `/readyz` before starting the next:

```sh
curl -fsS localhost:8082/readyz
```

A PDP isn't ready until it has synced policy and principals, so a 503 for the first seconds is
normal; persistent 503s mean an upstream (`AWZY_POLICY_ADDR`, `AWZY_CONTEXT_ADDR`) is unreachable.

## Clean up

Stop the services you started. Leave the stack running if someone else may be using it (other
sessions share it); otherwise `docker compose -f deploy/local/compose.yaml down`. Add `-v` only
to wipe the data on purpose.
