# 0055. Services in an install authenticate to each other with Kubernetes service-account tokens

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
- **A caller is a namespace and service account**, `awzy-system/awzy`. Each receiver groups its
  routes in code, by what they do, and the chart fills each group with the callers it allows,
  since only the chart knows the install's namespace names. A group the chart leaves empty is
  closed.
  - awzy-context: *readers* (principals, resources, work items: the PDP, connectors, the
    synthetic Job), *writers* (resources and work items: connectors, the synthetic Job), and
    *directory* (service accounts: the client portal, the synthetic Job). SCIM keeps the IdP's
    bearer token.
  - awzy-policy: bundles and the mode for the PDP; drafts and publishing for the client portal.
  - awzy-pdp: any service account in the cluster may ask for a decision. Enforcement points
    run anywhere, and Cedar decides what they may do; the token only proves who's asking.
- **Today the chart runs awzy's services under one service account per namespace** (`awzy`,
  bound to the cloud's workload identity in the install contract), so the line this draws is
  between the install's backend, its portal, and everything else in the cluster. That closes the
  open door: any pod in the client's cluster could read the directory or write resources.
  Telling the PDP from awzy-policy needs a service account per service, which also means a
  workload-identity binding per service (least-privilege cloud access); that's a change to the
  install contract, decided separately.
- **On in Kubernetes, off elsewhere.** Only pods have service-account tokens, so a service
  running on a laptop or in a test neither sends nor checks them, and says so at startup.
  `AWZY_SERVICE_AUTH=on|off` overrides that; the chart never sets it.
- Network policies stay, as a second fence.

Rejected: a service mesh with mTLS (Istio, Linkerd). It's more software to run and upgrade
in every client's cluster, and its identities are the same service accounts.
Rejected: shared static secrets per pair of services. They don't rotate and multiply with
every pair. Rejected: the TokenReview API. It costs a call to the API server per new token and
a cluster-wide RBAC grant, for the same answer local verification gives.

## Consequences

- Each service needs a projected token mounted per audience it calls and, to read the cluster's
  keys, the `system:service-account-issuer-discovery` role, which the chart binds.
- Until services have their own accounts, a compromised awzy backend pod can call anything the
  backend may; the namespace boundary is what this buys now.
- Callers from outside the cluster (an SDK in an app elsewhere) need another credential. That's
  decided when the first one exists.
- Rolled out per service: awzy-context first, then awzy-policy and awzy-pdp. Until a receiver
  checks, its callers' tokens are harmless.
