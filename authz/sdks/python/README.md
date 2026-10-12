# awzy-authz-sdk

awzy's Python client library: an enforcement point in your own app or agent (ADR 0045, 0114). It
validates callers' tokens, asks awzy-pdp within a budget, applies the mode, surfaces challenges as
a result to wait on, falls back as awzy-gateway does, and records activity. It's held to the same
conformance spec as every awzy client library, `authz/sdks/conformance/scenarios.json`.

`pyproject.toml` is the source of truth for its dependencies. Bazel installs them from
`requirements_lock.txt`, made from it, and runs the tests:

```sh
bazel test //authz/sdks/python:unit
pip-compile --extra test --generate-hashes --strip-extras --output-file requirements_lock.txt pyproject.toml
pip install -e '.[test]' && ruff check . && mypy src
```

How to use it is on the docs site's Client libraries section.
