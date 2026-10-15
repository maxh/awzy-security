# 0126. Protocols between services are modeled in P, beside the code, and checked in CI

- Status: Accepted
- Date: 2026-10-14
- Amends: [0054](0054-challenges-and-containments-over-grpc.md) (a replica vouches for its
  containment list only while it can read Postgres) and
  [0119](0119-a-pdp-outlasts-its-control-plane.md) (a PDP never goes back to an older list)

## Context

awzy's riskiest behavior sits between processes: a feed and the PDPs following it, signed state
that must only move forward, challenges answered while an agent retries, an outage fallback that
replays allows. Unit and integration tests run one interleaving each, the one the test happens
to produce. The bugs are in the others: a reconnect to the replica that's behind, a heartbeat
from a replica that can't see Postgres, a peer's answer arriving after the feed's.

AWS has written about this for a decade. TLA+ found bugs in S3 and DynamoDB that review and
testing had missed. Many teams there moved to P, a language of communicating state machines
that reads like the code it models; S3's move to strong consistency was checked in it. Their
practice puts the formal model on the design and keeps the code honest with tests and
production traces.

## Decision

- **Protocols get a P model.** Wherever correctness depends on the order of messages, on
  failures, or on time between services (a feed, version ordering, a lifecycle several services
  share, a fallback), the protocol has a model of its machines and the properties it promises.
  Logic inside one process is tested in Rust, with `proptest` where inputs vary; Cedar's
  evaluation has its own proofs and isn't modeled again.
- **P, not TLA+.** Its machines map to the services and tasks in the code, each event to a
  message on the wire, so a reviewer can hold the model against the Rust. Its checker explores
  schedules systematically and prints the trace that breaks a property.
- **Each machine sits beside the Rust it models**, with the same name: `feed.p` beside `feed.rs`,
  `containments.p` beside `containments.rs`. What the protocol's machines share (its events,
  timings, properties, the test that drives them, and the `.pproj`) is in a folder named for the
  protocol beside the module of the service that owns it, such as
  `authz/services/challenge/src/feed/`.
- **Properties are spec machines named as statements** (`BoundedStaleness`, `NoRollback`,
  `Converges`), one test case each, so a failure names the promise it broke. Timings mirror the
  Rust constants, scaled down so the checker reaches them, with the constant's name beside each.
- **CI checks every model.** The `p-models` job compiles each `.pproj` in the repository and
  runs each of its test cases for a fixed number of schedules; a broken property fails the run
  with the trace in the log. The job installs .NET and the P toolchain at a pinned version. It
  stays outside Bazel, as `buf` does, because Bazel has no .NET rules worth adding for one tool.
- **A change to code a model covers changes the model in the same pull request**, and a bug a
  model finds gets a Rust test that reproduces it.

The first model is the containment feed. Against the code as it was, it found three bugs, fixed
with it:

- **A follower could go back to an older list.** A PDP or gateway that reconnected to a replica
  that hadn't yet reloaded loaded that replica's older list, forgetting a containment it had
  enforced. Now a PDP and a gateway refuse a list older than the one they hold.
- **A replica that couldn't read Postgres kept vouching for its list.** Its heartbeats kept every
  PDP on it fresh, with no limit, while the other replica took containments. Now a replica
  sends its list and heartbeats only while it has read Postgres within `CONFIRMED_FOR`
  (10 seconds, two reloads). Its streams go quiet past that, so their PDPs go stale and ask
  awzy-challenge directly.
- **A heartbeat freshened whatever list the PDP held.** After a refused list, or a list that went
  to a stream while its replica was quiet, a heartbeat for a newer version made the PDP's older
  list look current. Now a heartbeat freshens only the version it names, and a stream that missed
  a list gets it as soon as its replica vouches again.

What the model now holds: a PDP never says a principal isn't contained from a list that names
them, or from a list Postgres replaced more than `CONFIRMED_FOR` plus
`AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS` ago; no PDP or gateway goes back to an older list; and once
failures stop, every one has the latest list.

## Consequences

- The model checks the design, not the Rust; the two can drift. Reviewers hold them together,
  with each machine beside its file. Checking production logs against a model (AWS's PObserve)
  can come later, once the services log the events a model names.
- CI takes a few more minutes, in a job of its own that runs alongside the others.
- While awzy-challenge's Postgres is down for more than 10 seconds, as in a failover, every
  replica goes quiet, so the PDPs' feeds go stale and the stale-feed alert pages. They serve the
  last list as before, up to their limit, and decisions say they were made on it. Before this,
  the heartbeats hid the outage.
- A replica's backstop reload is now an interval. Under a sleep that each challenge notification
  restarted, a busy install never reloaded, and with this change it would stop vouching.
- A Postgres restored to an older feed version leaves PDPs and gateways refusing the restored
  lists until they restart. The PDPs ask awzy-challenge on every check meanwhile, and the
  freshness alert fires. Restarting them is part of restoring awzy-challenge's database.
- Working on a model needs .NET 8 and `dotnet tool install --global P`; `DEVELOPMENT.md` says how.
