# 0054. Services in an install authenticate to each other with Kubernetes service-account tokens

- Status: Accepted
- Date: 2026-10-10

## Context

Inside an install, awzy's services call each other with no credentials: the PDP loads people,
groups, and resources from awzy-context; the client portal publishes policy through awzy-policy
and asks the PDP about its own users; the demo-data Job writes the synthetic company into
awzy-context. Anything that can reach those ports can read the directory, write resources,
or publish policy. Network policies are the only fence, and three TODOs say the decision is
open. That has to close before an install holds a client's data.

Every client runs Kubernetes, and every Kubernetes issues each workload a signed, short-lived
token naming it (`system:serviceaccount:<namespace>:<name>`) and publishes the keys to verify
it. awzy should run as little extra software in a client's cluster as it can (platform plan).

## Decision

- **Callers present a projected service-account token** as `Authorization: Bearer`, with the
  audience set to the service they call (`awzy-context`, `awzy-policy`, `awzy-pdp`). The
  chart mounts one projected token per audience a workload needs; the kubelet rotates them
  (10-minute lifetime), and clients read the file on each call.
- **Receivers verify locally**: the signature against the cluster's keys
  (`https://kubernetes.default.svc/openid/v1/jwks`, fetched with the pod's own token and the
  cluster CA, refetched when a token names an unknown key), `aud` equal to their own name, `exp`,
  and `iss` equal to the issuer in the pod's own token. No call to the API server per request,
  so the PDP's hot path doesn't grow. The library is `authz/libs/service-auth`.
- **Each receiver decides who may call what**, by service account and namespace, in code:
  - awzy-context: the PDP reads principals and resources; connectors and the synthetic Job
    write resources and work items; the client portal manages service accounts. SCIM keeps
    the IdP's bearer token.
  - awzy-policy: the PDP reads bundles and the mode; the client portal drafts and publishes.
  - awzy-pdp: any service account in the cluster may ask for a decision. Enforcement points
    run anywhere, and Cedar decides what they may do; the token only proves who's asking.
  - Allowed callers must be in the install's own namespaces (`AWZY_SERVICE_AUTH_NAMESPACES`),
    so a workload elsewhere in the cluster can't pass by naming its account `awzy-pdp`.
- **`AWZY_SERVICE_AUTH=off`** turns sending and checking off for local development and tests,
  with a warning at startup. The chart never sets it.
- Network policies stay, as a second fence.

Rejected: a service mesh with mTLS (Istio, Linkerd). It's more software to run and upgrade
in every client's cluster, and it proves which pod is calling but not which awzy component.
Rejected: shared static secrets per pair of services. They don't rotate and multiply with
every pair. Rejected: the TokenReview API. It costs a call to the API server per new token and
a cluster-wide RBAC grant, for the same answer local verification gives.

## Consequences

- Each service needs its audience's projected token mounted and, to read the cluster's keys,
  the `system:service-account-issuer-discovery` role, which the chart binds.
- Callers from outside the cluster (an SDK in an app elsewhere) need another credential. That's
  decided when the first one exists.
- Rolled out per service: awzy-context first, then awzy-policy and awzy-pdp. Until a receiver
  checks, its callers' tokens are harmless.
