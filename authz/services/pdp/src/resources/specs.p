// What the cache promises while the change log is heard from.

// A check decides on a resource's attributes no more than ChangeTarget() after a newer version
// committed.
spec ChangesReachChecks observes eCommitted, eUsed {
  var committedAt: map[int, int];
  var latest: int;

  start state Watching {
    entry { latest = 1; }
    on eCommitted do (c: (version: int, at: int)) {
      committedAt[c.version] = c.at;
      latest = c.version;
    }
    on eUsed do (u: (version: int, at: int)) {
      if (u.version >= latest) { return; }
      assert u.at - committedAt[u.version + 1] <= ChangeTarget(),
        format("at {0}, a check used version {1}, though version {2} committed at {3}",
          u.at, u.version, u.version + 1, committedAt[u.version + 1]);
    }
  }
}
