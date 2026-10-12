# 0114. Client libraries in four languages, held to one conformance spec

- Status: Accepted
- Date: 2026-10-12
- Amends: [0045](0045-sdks-are-enforcement-points-built-on-a-stable-proto.md) (one SDK at a time)
- Builds on: [0112](0112-the-typescript-sdk.md) (the TypeScript SDK)

## Context

0045 said to build one SDK at a time, each passing the conformance suite before the next was
started, so that a wrong behavior was fixed once rather than ported. The TypeScript SDK (0112) now
exists, and the next design partners' apps and agent frameworks are in Python, Go, and Java or
Kotlin. Waiting for each in turn would leave them calling the PDP's API directly, which 0045
names as the way enforcement quietly fails open.

What 0045 feared from several at once is drift: libraries that disagree on a mode, a fallback, or
an answer they can't read. Its suite was to be the guard, but so far it lived inside the
TypeScript tests.

## Decision

- **Client libraries for Python, Go, and Java (usable from Kotlin), alongside TypeScript**, in
  `authz/sdks/<language>`. The docs call them client libraries; each keeps 0045's rules.
- **One conformance spec, `authz/sdks/conformance/scenarios.json`**: each scenario is what the
  PDP answers (or that it's down or slow), the call made, and the result every library must give.
  Every library's tests read the same file. A behavior is changed by changing the spec, and every
  library's CI then fails until it follows.
- **Each library builds with its own language's tools** (pnpm, pip, the Go toolchain, Maven), in
  a CI workflow per language, because that's how the apps using it build it. TypeScript keeps its
  Bazel targets, which already existed.
- **Names say what they are**: `@awzy/authz-sdk` on npm, `awzy-authz-sdk` on PyPI (imported as
  `awzy_authz`), the Go module at `authz/sdks/go`, and `com.awzysecurity:awzy-authz-sdk` for Java.

Token validation isn't in the spec: each language validates with its own JOSE library, and each
library has its own tests for the same rules (`act`, client credentials, a subject claim, and
apps' own agents).

## Consequences

- Four libraries to keep equal. The spec makes a disagreement a failing build rather than a
  report from the field.
- A library is listed in the docs once it passes the whole spec.
- 0045's rule that the protobuf is a public contract is unchanged; the libraries speak the PDP's
  JSON API (0112), which is held to the same rule.
