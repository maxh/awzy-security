# 0130. A run's logs are replayed against its P model's properties

- Status: Accepted
- Date: 2026-10-15
- Amends: 0126

## Context

The P models (0126) check the design, and 0126 accepts that the Rust can drift from them:
reviewers hold the two together by hand. AWS checks production traces against its P models
(PObserve) to close that gap. The model's properties are already written as spec machines over
announced events, so anything that can announce the same events in the same order can be
checked by the same specs.

## Decision

- **Services log the events their model names**, as tracing events at debug level under the
  `awzy_model` target (`awzy_telemetry::MODEL_EVENTS`), with a `model_event` field holding the P
  event's name and its payload fields under the names `events.p` gives them. Off by default:
  `RUST_LOG=info,awzy_model=debug` turns them on, and a service skips any work they need, such as
  reading the whole list, when they're off.
- **`tools/ci/replay-p-model.py` replays them.** It reads the JSON lines from any number of
  services' logs, orders them by time, and writes a P machine that announces each in turn. The
  model's own `specs.p` observes it, through tests in the model's `replay/` folder, with the real
  timings in milliseconds. A replay has no choices, so one schedule checks the whole run. The
  payload types come from `events.p`, so a new model's replay needs only its `replay/` folder.
- **Only properties of any stretch of a run are replayed.** Logs can start mid-run, and a run
  can end before failures heal, so a property like the feed's Converges stays with the model. A
  line about a version older than any the logs hold comes from before they began, and is skipped.
- **The containment feed is first.** awzy-challenge logs `eStoreChanged` when a change commits,
  with the list the new version has, and the list as each replica first reads it. The PDP logs
  `eApplied` for each list it loads and `eDecision` for each answer it gives from its list, under
  a per-process instance ID, so a restarted PDP is a new follower. Answers from a direct lookup
  aren't logged: they come from Postgres, which has no stale list to answer from.
- **Times come from different hosts' clocks.** The replay's `Delivery()` (1 s for the feed) covers
  their skew as well as the stream's latency.
- **CI checks the replay itself** on fixture logs, each of which must pass every test or break
  the one it names, as the models' `bugs.p` keeps each model catching what it caught.

## Consequences

- A run's logs can be checked against the design: an integration test or a game day (#402), and
  later an install, as a monitor. Collecting a game day's logs with model events on is the next
  step; this decision gives the format and the checker.
- A test holds each logged event to the fields `events.p` declares, so renaming a field in the
  model without the Rust, or the reverse, fails a test rather than a replay.
- Logging every decision is too much for production at full rate. An install would sample, or
  log only decisions made while the feed is stale, where the property has something to check.
