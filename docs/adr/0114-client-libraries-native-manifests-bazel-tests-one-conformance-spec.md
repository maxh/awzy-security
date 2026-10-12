# 0114. Client libraries: native manifests, Bazel tests, and one conformance spec

- Status: Accepted
- Date: 2026-10-12
- Amends: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (one SDK at a time)
- Builds on: [0112](0112-the-typescript-sdk.md) (the TypeScript SDK)

## Context

0045 said to build one SDK at a time, each passing the conformance suite before the next was
started, so that a wrong behavior was fixed once rather than ported. The TypeScript SDK (0112) now
exists, and the next design partners' apps and agent frameworks are in Python and Go. Waiting for
each in turn would leave them calling the PDP's API directly, which 0045 names as the way
enforcement quietly fails open.

What 0045 feared from several at once is drift: libraries that disagree on a mode, a fallback, or
an answer they can't read. Its suite was to be the guard, but so far it lived inside the
TypeScript tests.

The libraries also sit between two build worlds. Everything in this repository builds with Bazel,
but each library is published into its own language's ecosystem, where apps install it with npm,
pip, or `go get`, and those read the language's own manifest.

## Decision

- **Client libraries for TypeScript, Python, and Go**, in `authz/sdks/<language>`. The docs call
  them client libraries; each keeps 0045's rules. Java (usable from Kotlin) is next, in the same
  shape, when a partner needs it.
- **Each library's native manifest is the source of truth** for its dependencies and what's
  published: `package.json` and the pnpm lockfile, `pyproject.toml` and a lockfile generated from
  it (`requirements_lock.txt`), and `go.mod` with `go.sum`. Nobody lists a dependency twice.
- **Bazel builds and tests each one by reading that manifest**, as `rules_js` already reads the
  pnpm lockfile for the TypeScript SDK: `rules_python` installs from the Python lockfile, and
  `rules_go` resolves modules from `go.mod`. So `bazel test //...` runs every library's tests with
  the rest of awzy, cached alike, and a library that breaks fails the same build as a service.
- **Each language's own linters run in CI beside the build**, as oxlint does for TypeScript: ruff
  and mypy for Python, gofmt and `go vet` for Go.
- **One conformance spec, `authz/sdks/conformance/scenarios.json`**: each scenario is what the
  PDP answers (or that it's down or slow), the call made, and the result every library must give.
  Every library's tests read the same file. A behavior is changed by changing the spec, and every
  library's tests then fail until it follows.
- **Names say what they are**: `@awzy/authz-sdk` on npm, `awzy-authz-sdk` on PyPI (imported as
  `awzy_authz`), and the Go module at `github.com/maxh/awzy-security/authz/sdks/go` (package
  `awzyauthz`).

Token validation isn't in the spec: each language validates with its own JOSE library, and each
library has its own tests for the same rules (`act`, client credentials, a subject claim, and
apps' own agents).

Rejected: building the libraries only with their native tools, outside Bazel. Their tests wouldn't
run in `bazel test //...`, so a change to the spec or the PDP's JSON could pass the main build and
break a library. Rejected: Bazel as the source of truth for their dependencies. The published
packages need native manifests anyway, and keeping a second list in sync is how they'd drift.

## Consequences

- Three libraries to keep equal; the spec makes a disagreement a failing build rather than a
  report from the field.
- `MODULE.bazel` gains `rules_python` and `rules_go`, with the Python and Go toolchains they
  fetch. Changing a Python dependency means regenerating its lockfile from `pyproject.toml`.
- A library is listed in the docs once it passes the whole spec.
- 0045's rule that the protobuf is a public contract is unchanged; the libraries speak the PDP's
  JSON API (0112), which is held to the same rule.
