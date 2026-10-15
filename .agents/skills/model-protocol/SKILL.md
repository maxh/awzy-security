---
name: model-protocol
description: Model a protocol between awzy's services in P and check it (ADR 0126), or change a model when the code it covers changes. Use when adding or changing a feed, a version-ordered sync, a lifecycle several services share, a fallback, or any file with a `.p` beside it.
---

# Model a protocol in P

ADR 0126 has the why. The containment feed is the worked example:
`authz/services/challenge/src/feed/` (events, timings, properties, test, `.pproj`) with
`store.p`, `feed.p`, and the PDP's and gateway's `containments.p` beside their `.rs`.

1. **Decide it needs one.** It does when correctness depends on message order, failures, or
   time between processes. Logic inside one process gets a Rust test instead (`proptest` when
   inputs vary).
2. **Lay it out.** Each machine goes in a `.p` beside the Rust it models, with the same base name.
   The shared files go in a folder named for the protocol beside the owning service's module:
   `events.p`, `timings.p` (the Rust constants, scaled down, each with its Rust name), `specs.p`,
   `test.p`, and `<Name>.pproj` listing every file by relative path.
3. **Model the code as it is first.** Faithful to the Rust: the same retries, timeouts, and
   checks, including the ones you suspect. Abstract what doesn't matter (payload contents,
   anything past the protocol). Time is ticks that every machine acknowledges, as in the feed's
   `test.p`.
4. **Write the properties** as `spec` machines named as statements, observing announced events,
   with one `test` case per spec so a failure names the promise. Prefer safety ("never") and a
   bounded "by the end, once failures stop" over unbounded liveness.
5. **Check it:** `AWZY_P_SCHEDULES=200 tools/ci/check-p-models.sh path/to/Name.pproj` while
   iterating, then the default 3000. Setup is in DEVELOPMENT.md (P models). A failure prints
   the trace. Decide whether it's a bug in the code or in the model.
6. **Fix bugs in both.** Change the model to the fixed design, change the Rust to match, and add a
   Rust test that reproduces each bug, named for the behavior. Record the bugs found and any
   change to a decision in an ADR (see `write-adr`).
7. **Keep each bug behind a switch.** The protocol's folder has a `bugs.p` with
   `fun Bug(): int { return 0; }` and a line per bug, `// bug <n> breaks <test>: <what it was>`.
   Where the fix is in the model, keep the old behavior when `Bug() == <n>`. CI puts each back
   and fails unless its test breaks. One the checker needs more than 3000 schedules to reach
   says how many, `breaks <test> within <n>:`, and only the nightly run checks it. A bug the
   checker never reached by itself gets a note in `bugs.p` instead of a line.
8. **Replay real runs** (ADR 0130). Log each event the specs observe from the Rust, at debug
   under `awzy_telemetry::MODEL_EVENTS`, with `model_event` and the payload's field names from
   `events.p`, and add a test that holds the lines to them (the feed's is in
   `challenge/src/grpc/tests.rs`). Then a `replay/` folder: `replay.json`, `timings.p` in real
   milliseconds, `tests.p` for the properties of any stretch of a run, and fixture logs that pass
   or break each. `tools/ci/replay-p-model.py --self-test` checks them.
9. **Keep it in step.** A PR that changes a file with a `.p` beside it changes the model, or says
   in its description why it doesn't need to. CI warns when it doesn't.
