# 0093. Every image and release carries a signed SBOM

- Status: Accepted
- Date: 2026-10-11

## Context

Releases are signed images and a signed chart (0057), and Kyverno admits only signed images into
awzy's namespaces. The platform plan also promises an SBOM with each release, so a client can see
what they run and match it against advisories, and none was published.

awzy's images are distroless with a statically linked Rust binary, which carries no package
metadata: an SBOM made from the image lists the base image's packages and not the crates
compiled in.

## Decision

- **Each image's SBOM** is made by syft, from the OCI layout Bazel built (or, for a mirrored
  third-party image, the image in the registry), in SPDX JSON, and attached with `cosign attest
  --type spdxjson`, signed by the release key. Mirrored images get theirs once, like their
  signatures.
- **The release's SBOM** lists every Rust crate and npm package the release is built from, from
  `Cargo.lock` and `pnpm-lock.yaml`, attested the same way on the chart. It's what an image's
  SBOM can't show.
- **Kyverno requires the image SBOM:** the verify policy admits an image only with an SPDX
  attestation signed by the release key, beside its signature.

## Consequences

- An image published before this has no SBOM, so Kyverno would refuse a pod made from it. Apply
  bootstrap's new policy only once an install runs a release published with SBOMs; mirrored
  images get theirs on the next publish.
- The release SBOM lists what the whole release builds from, so it names more crates than any one
  binary links.
- Publishing makes one more signature per image, and an `.att` tag beside each `.sig`.
