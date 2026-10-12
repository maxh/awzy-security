# 0119. A PDP outlasts its control plane: state from its peers, and a limit on how stale each input may be

- Status: Accepted
- Date: 2026-10-12
- Amends: [0044](0044-pdp-loads-resources-from-awzy-context.md) (stale resources had no limit),
  [0054](0054-challenges-and-containments-over-grpc.md) (a stale containment feed failed every
  check), and [0021](0021-signed-policy-bundles.md) (a PDP that never loaded a bundle stays
  not ready)

## Context

A PDP decides from memory, but it fills that memory from three services: the bundle and modes
from awzy-policy, the principals snapshot and resources from awzy-context, and the containment
list from awzy-challenge. All three run on the install's one Postgres.

- **Starting** (#346): a PDP is ready only once all three have answered. Pods start on every
  rollout, scale-up, node replacement, and eviction, so a Postgres outage becomes a PDP outage
  the first time the cluster replaces a pod. Enforcement points then use their outage fallback,
  which fails closed for anything it doesn't list.
- **Running** (#347): two inputs fail checks within seconds. Once the containment feed misses
  2 seconds of heartbeats, every check asks awzy-challenge directly with a 250 ms timeout, so
  restarting awzy-challenge fails every check, and each one waits out the timeout first. A
  resource the cache doesn't hold fails when awzy-context can't be reached. A resource it does
  hold is the opposite: once expired, it's served as long as awzy-context stays unreachable,
  with no limit.

A managed Postgres failover takes one to two minutes. With two PDP replicas and an autoscaler,
that's long enough for the cluster to replace a pod.

## Decision

**Each input has a written dependency and a limit.** What the PDP needs, and what it does
without its source:

| Input | Source | Source unreachable, PDP running | Source unreachable, PDP starting |
| --- | --- | --- | --- |
| Bundle and modes | awzy-policy | Keeps what it has; no limit | From a peer, verified |
| Principals snapshot | awzy-context | Keeps what it has; no limit | From a peer |
| Resources | awzy-context | A cached entry is served up to its tier's limit; a miss asks a peer | Misses ask a peer |
| Containments | awzy-challenge | The last list is used up to 5 minutes after the feed was last heard, then checks fail | From a peer, with the peer's age |

- **Policy and principals have no limit** because their changes have one way in. A bundle is
  published, and a person deprovisioned, only through the service that's down, so nothing newer
  can exist until it's back. Failing every check because the directory is ten minutes old
  would turn an awzy-context outage into a total one. The freshness alerts already page after
  60 seconds.
- **Containments have a limit** because they're the response to an incident in progress, and
  awzy-challenge can be up while one PDP's feed is cut. When the feed is stale the PDP still asks
  awzy-challenge directly, as before. If that fails, it uses the last list, and for the next
  second it doesn't ask again, so an outage doesn't add 250 ms to every check. After
  `AWZY_PDP_CONTAINMENTS_MAX_STALE` (5 minutes by default, past a failover) checks fail as
  unavailable, never as "not contained."
- **Resources have a limit by tier**, since a stale attribute matters more on a sensitive
  resource: `public` and `internal` 60 minutes, `confidential` 15, `crown_jewels` 5 (chart
  settings). Past it, the check fails as unavailable, as a miss does now. The cache drops entries
  past their limit before it drops anything else. A miss still never falls back to the default
  tier (0044).

**A starting PDP takes its state from a running one.** A new internal service,
`awzy.pdp.v1.PdpPeerService`, outside the stable `awzy.authz.v1`, which enforcement points don't
use:

- `GetState` returns the bundle and modes as signed, the principals snapshot, and the containment
  list. `GetResource` returns one cached resource entry.
- A PDP that can't reach a source at start finds its peers through a headless Service,
  `awzy-pdp-peers`, asks up to five, and takes the newest of each input by version. It still
  verifies every bundle and mode against its verify keys, so a peer can't hand it policy
  awzy-policy didn't sign. It trusts the snapshot and the containment list as it trusts
  awzy-context and awzy-challenge: the peer is a pod of the PDP's own Service, and it answers
  only callers with the backend's service-account token (0055). The chart gives the method an
  access group of its own, `peers`, holding only the backend.
- Each input travels with its age in milliseconds, not a timestamp, so node clocks don't matter
  and a hop doesn't make old state look new. The receiver keeps ageing it, and the limits above
  apply to that age.
- When a source answers, it replaces what came from a peer. 0080's rule still holds: nothing
  older replaces what a PDP has applied.

**A degraded decision says so.** A decision made on an input that is past its normal freshness
(policy or principals older than 60 seconds, containments older than 2 seconds, a resource past
its TTL) records which ones in a new decision log field, `stale_inputs`
(`containments`, `principals`, `policy`, `resources`). The client portal shows it on the
decision page. The PDP's freshness metrics report the inputs' ages, not the time since the
PDP's own last sync, so a PDP started from a peer alerts as its peer would.

Rejected: last-known-good state in the install's bucket. It covers a cold start, but the
principals snapshot and containment list aren't signed, so awzy-context and awzy-challenge would
each need a KMS key and a workload identity of their own, and every cloud an object-store
client. Rejected: a persistent volume per PDP. It ties pods to zones and nodes, and a
replacement pod starts on a new volume anyway. Rejected: failing closed on any stale input, as
now. It makes the PDP's availability the product of Postgres, awzy-context, and awzy-challenge,
and the PDP is the one thing in an install that must stay up.

## Consequences

- A Postgres failover, or a restart of awzy-policy, awzy-context, or awzy-challenge, no longer
  fails checks or blocks new PDPs, provided one PDP is running. The PodDisruptionBudget already
  keeps one.
- A cold start, with every PDP gone while a source is down, still leaves PDPs not ready, and
  enforcement points still use their fallback. The game day (#353) measures how often that
  happens.
- During an awzy-challenge outage a new containment takes up to 5 minutes, not 2 seconds, to
  reach a PDP whose feed is cut, and the containment feed alert pages for all of it.
- Resources are no longer served stale without limit. A long awzy-context outage now fails
  checks on crown-jewel resources after 5 minutes, where before it kept serving their old
  attributes.
- The decision log gains a ClickHouse column, and the PDP a new proto, a headless Service, and an
  access group.
