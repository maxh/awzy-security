// Connector writes, each beginning and committing whenever it likes within MaxWrite() ticks,
// possibly out of order, and a check every tick. A transaction that never ends would hold the
// change log back for as long, and isn't one awzy-context runs.

fun Rounds(): int { return 14; }
fun MaxWrite(): int { return 2; }

machine ResourceClock {
  var db: machine;
  var cache: machine;
  var checker: machine;
  var follower: machine;
  var now: int;
  var begun: set[int];
  var begunAt: map[int, int];
  var writes: int;

  start state Running {
    entry {
      var w: int;
      db = new ContextDb();
      cache = new ResourceCache();
      checker = new ResourceChecker((cache = cache, db = db));
      follower = new ChangeFollower((cache = cache, db = db));
      while (now < Rounds()) {
        now = now + 1;
        send db, eTick, (clock = this, now = now);
        receive { case eTickAck: {} }
        // Writes begin and commit, possibly out of order, before and while the check runs.
        if (writes < 4 && $) {
          send db, eBegin, writes;
          begun += (writes);
          begunAt[writes] = now;
          writes = writes + 1;
        }
        foreach (w in begun) {
          if (now - begunAt[w] >= MaxWrite()) {
            begun -= (w);
            send db, eCommit, w;
          }
        }
        // The check starts, and a write may commit while it's fetching.
        send checker, eCheck, (clock = this, now = now);
        if (sizeof(begun) > 0 && $) {
          w = choose(begun);
          begun -= (w);
          send db, eCommit, w;
        }
        send follower, eTick, (clock = this, now = now);
        receive { case eTickAck: {} }
        receive { case eTickAck: {} }
      }
    }
  }
}

test tcChangesReachChecks [main = ResourceClock]:
  assert ChangesReachChecks in { ResourceClock, ContextDb, ResourceCache, ResourceChecker, ChangeFollower };
