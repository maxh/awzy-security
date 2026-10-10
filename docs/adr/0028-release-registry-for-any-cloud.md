# 0028. A release registry installs can pull from on any cloud

- Status: Accepted (option B, 2026-10-09)
- Date: 2026-10-09
- Answers the open question in [0017](0017-test-installs-on-gcp-early.md); builds on
  [0018](0018-awzysecurity-domain-and-dns.md) and [0026](0026-install-registry-and-heartbeats.md)

## Context

Releases are published to ECR in `awzy-build` (signed images and the chart). An install pulls
in three places: the kubelet (images), Argo CD (the chart), and Kyverno (signatures). Today the
example install gets in through:

- an ECR repository policy naming its AWS account, and an exception for its organization in
  awzy's RCP;
- IAM in the install's own account (`ecr:GetAuthorizationToken`);
- a CronJob in `bootstrap/` that refreshes the 12-hour ECR token for Argo CD and Kyverno.

None of that works for an install on GCP or Azure: their nodes have no AWS identity, and the
token can't be refreshed without one. It also ties every client to an AWS account ID in awzy's
config and makes `bootstrap/` carry ECR-specific code.

What a release registry has to do:

1. Pull works from any cloud, and from a client's own mirror, with no AWS identity.
2. Each install has its own pull credential that awzy can revoke, and that doesn't expire on a
   schedule installs have to chase.
3. A stable name, `registry.awzysecurity.com`, so the backend can change without touching
   installs.
4. Signatures (cosign, 0009) and the chart are served the same way as images.
5. awzy runs as little as possible, and a registry outage doesn't take down running installs.

## Options

**A. Public images on a public registry** (for example ECR Public or GHCR), still cosign-signed.
Pulls need no credentials at all, mirroring is trivial, and nothing is left to operate. The cost
is that anyone can download awzy's binaries; access to the product is then controlled by the
license and the corp portal registry (0026), not by the registry. Many single-tenant vendors do
this.

**B. A pull proxy at `registry.awzysecurity.com`** in front of ECR: a small service in awzy's
account that checks a per-install pull token (issued when the install is registered, 0026),
serves manifests from ECR, and redirects layer downloads to ECR's presigned URLs. ECR, scanning,
and signing stay as they are. awzy runs one more service on the pull path.

**C. A hosted registry with per-install robot accounts** (for example Quay, Docker Hub, or
Cloudsmith), with a custom domain if the provider supports one. Static, revocable credentials
without awzy running a service, but a second registry to publish to and pay for, and each
provider differs in custom domains and token limits.

**D. Keep ECR and add per-cloud credential helpers.** Rejected: it's the problem in 0017, once
per cloud.

## Decision

**B: a pull proxy at `registry.awzysecurity.com` in front of ECR.** Release images stay private
(decided 2026-10-09), which rules out A.

- `awzy-registry` is a small service in awzy's account, next to the corp portal. It implements
  pull only of the OCI distribution API: token auth, manifests served from ECR, and layer
  downloads redirected to ECR's presigned URLs, so layer bytes never pass through it.
- Each install gets its own pull token when it is registered in the corp portal (0026): shown
  once, stored as a SHA-256 hash, revocable there. It is a separate credential from the
  reporting token, and it can only read `awzy/*` images, signatures, and the chart.
- Installs refer only to `registry.awzysecurity.com/awzy/...`. The kubelet uses an
  imagePullSecret, Argo CD a repository secret, and Kyverno a registry secret, all from the same
  static token, so nothing in an install refreshes credentials.
- `bootstrap/` takes the token as an input and drops the ECR refresher CronJob. Removed with it:
  the `registry-auth` role in `aws/install`, the account grants in `corp/build`, and the
  example client's RCP exception in `corp/org`.
- Production installs are told to mirror into their own registry (pull-through cache), so a
  proxy outage only delays upgrades and new nodes, not running pods.

## Consequences

- awzy runs one more service on the pull path. It is stateless apart from token hashes, which it
  reads from the corp portal's database or an API, and it needs at least two replicas once real
  clients depend on it.
- The ECR-specific pieces stay in place until the proxy exists; the example install keeps using
  them meanwhile.
- Any cloud (and any client mirror) can pull with a standard registry credential, which is what
  the GCP test install (0017) needs.
