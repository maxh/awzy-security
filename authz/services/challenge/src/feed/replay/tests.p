// The properties of specs.p that hold of any stretch of a run, checked over its logged events
// (ADR 0130). Converges needs the run to end with the failures healed, so only the model checks it.
test tcReplayBoundedStaleness [main = Replay]:
  assert BoundedStaleness in { Replay, Follower };
test tcReplayNoRollback [main = Replay]:
  assert NoRollback in { Replay, Follower };
