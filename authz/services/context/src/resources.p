// awzy-context's resources and their change log (resources.rs, store.rs): a write's trigger
// inserts a row with the write's transaction ID, and the change log answers the rows from the
// caller's cursor up to the horizon, the oldest transaction still running.
machine ContextDb {
  var now: int;
  var version: int;
  // Each running write's transaction ID, taken at its first write.
  var running: map[int, int];
  var nextSeq: int;
  // Committed rows' transaction IDs.
  var committed: set[int];

  start state Serving {
    entry {
      version = 1;
      nextSeq = 1;
    }

    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      send t.clock, eTickAck;
    }

    on eBegin do (w: int) {
      running[w] = nextSeq;
      nextSeq = nextSeq + 1;
    }

    on eCommit do (w: int) {
      if (!(w in running)) { return; }
      committed += (running[w]);
      running -= (w);
      version = version + 1;
      announce eCommitted, (version = version, at = now);
    }

    on eFetch do (from: machine) { send from, eFetched, version; }

    // Every row whose transaction ID is below the horizon, the oldest transaction still running,
    // is final: committed or never to be. The cursor is that horizon, so a write that commits
    // late, with an older ID, is still ahead of it.
    on eChanges do (c: (from: machine, after: int)) {
      var s: int;
      var horizon: int;
      var changed: bool;
      horizon = nextSeq;
      foreach (s in values(running)) { if (s < horizon) { horizon = s; } }
      if (c.after >= 0) {
        foreach (s in committed) {
          if (s >= c.after && s < horizon) { changed = true; }
        }
      }
      send c.from, eChangesAnswer, (cursor = horizon, changed = changed, reset = false);
    }
  }
}
