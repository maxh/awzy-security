// A PDP's resource cache (resources.rs): checks fetch a resource on a miss and cache it, and a
// task of its own follows awzy-context's change log, dropping what changed. The two share the
// cache, so each is a machine here.

machine ResourceCache {
  var cached: bool;
  var version: int;
  var at: int;
  // Moves with each drop, so a fetch that began before one isn't cached after it.
  var epoch: int;

  start state Serving {
    on eGet do (from: machine) { send from, eEntry, (cached = cached, version = version, at = at); }
    on eStore do (s: (version: int, at: int, epoch: int)) {
      // What it fetched may be from before the change the drop was for.
      if (s.epoch != epoch) { return; }
      cached = true;
      version = s.version;
      at = s.at;
    }
    on eInvalidate do {
      cached = false;
      epoch = epoch + 1;
    }
    on eEpoch do (from: machine) { send from, eEpochIs, epoch; }
  }
}

// merge and lookup: a fresh entry answers; otherwise the resource is fetched and stored.
machine ResourceChecker {
  var cache: machine;
  var db: machine;

  start state Init {
    entry (c: (cache: machine, db: machine)) {
      cache = c.cache;
      db = c.db;
      goto Checking;
    }
  }

  state Checking {
    on eCheck do (c: (clock: machine, now: int)) {
      var e: (cached: bool, version: int, at: int);
      var v: int;
      var epoch: int;
      send cache, eGet, this;
      receive { case eEntry: (r: (cached: bool, version: int, at: int)) { e = r; } }
      if (e.cached && c.now - e.at < FeedTtl()) {
        v = e.version;
      } else {
        send cache, eEpoch, this;
        receive { case eEpochIs: (n: int) { epoch = n; } }
        send db, eFetch, this;
        receive { case eFetched: (f: int) { v = f; } }
        send cache, eStore, (version = v, at = c.now, epoch = epoch);
      }
      announce eUsed, (version = v, at = c.now);
      send c.clock, eTickAck;
    }
  }
}

// follow_changes: long-polls the change log and drops what it names.
machine ChangeFollower {
  var cache: machine;
  var db: machine;
  var cursor: int;

  start state Init {
    entry (c: (cache: machine, db: machine)) {
      cache = c.cache;
      db = c.db;
      cursor = -1;
      goto Following;
    }
  }

  state Following {
    on eTick do (t: (clock: machine, now: int)) {
      send db, eChanges, (from = this, after = cursor);
      receive {
        case eChangesAnswer: (a: (cursor: int, changed: bool, reset: bool)) {
          // The first answer only sets the cursor, so what was cached before it, or fetched
          // while it was asked, may predate a change it will never name: dropped too.
          if (a.changed || a.reset || cursor < 0) { send cache, eInvalidate; }
          cursor = a.cursor;
        }
      }
      send t.clock, eTickAck;
    }
  }
}
