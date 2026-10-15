# 0129. The containment feed's staleness bound includes the time a message takes to arrive

- Status: Accepted
- Date: 2026-10-15
- Amends: 0126

## Context

0126 says the containment feed's model holds this: a PDP never answers "not contained" from a
list Postgres replaced more than `CONFIRMED_FOR` plus `AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS` ago.
The `p-models` job then failed `tcBoundedStaleness` on a pull request that changed nothing it
covers: a PDP answered from a list 13 ticks after Postgres replaced it, against a bound of 12.

The code was doing what it was designed to do. A replica vouches for its list until
`CONFIRMED_FOR` after it last read Postgres. A PDP counts staleness from when the list or
heartbeat *arrives* (`Containments::heard`, on its own clock, since the two hosts' clocks can't
be compared). A message the replica sends at the end of its window reaches the PDP a moment
later, and the PDP's `MAX_STALE` starts from there. In the model, a list a replica sends in reply
to a subscribe or a read, rather than on a tick, can be handled in the next tick, so the model
reached this, though only once in many thousands of schedules.

## Decision

- **The bound includes delivery:** `CONFIRMED_FOR` plus the time the replica's last list or
  heartbeat takes to reach the PDP plus `MAX_STALE`. In the model that's one tick, `Delivery()`
  in `feed/timings.p`. In production it's the stream's latency, milliseconds against a bound of
  310 s.
- **No change to the code.** Sending the replica's confirmation time instead would need clocks
  the PDP and replica agree on, which 0119 avoids: state travels with its age, not a timestamp.

## Consequences

- `tcBoundedStaleness` no longer fails on schedules that deliver a list a tick late. Bugs 2 and 3,
  which it catches, still let a list stand far past the new bound.
- A message held up in transit stretches the bound by as long as it was held. The heartbeats
  behind it are held too, so the PDP is stale meanwhile and asks awzy-challenge directly; but the
  list it holds counts as fresh again when the held message arrives.
