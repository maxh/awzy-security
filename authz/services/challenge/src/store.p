// awzy-challenge's Postgres: the feed version and the active containments. Every change bumps
// the version and notifies the replicas listening, in the same transaction.
machine Store {
  var now: int;
  var up: bool;
  var version: int;
  var contained: tContained;
  var replicas: seq[machine];
  // Replicas cut off from Postgres: their reads fail and their listener hears nothing.
  var cut: set[machine];

  start state Init {
    entry {
      up = true;
      version = 1;
      receive {
        case eReplicas: (r: seq[machine]) { replicas = r; }
      }
      announce eStoreChanged, (version = version, contained = contained, at = now);
      goto Serving;
    }
  }

  state Serving {
    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      send t.clock, eTickAck;
    }

    on eWrite do (w: (principal: int, contain: bool)) {
      var r: machine;
      if (!up) { return; }
      if (w.contain == (w.principal in contained)) { return; }
      if (w.contain) { contained += (w.principal); } else { contained -= (w.principal); }
      version = version + 1;
      announce eStoreChanged, (version = version, contained = contained, at = now);
      foreach (r in replicas) {
        if (!(r in cut)) { send r, eNotify, version; }
      }
    }

    on eRead do (from: machine) {
      if (up && !(from in cut)) {
        send from, eReadResult, (ok = true, version = version, contained = contained, at = now);
      } else {
        send from, eReadResult, (ok = false, version = 0, contained = default(tContained), at = now);
      }
    }

    on eLookup do (l: (pdp: machine, replica: machine, principal: int)) {
      if (up && !(l.replica in cut)) {
        send l.pdp, eLookupResult, (ok = true, version = version, contained = l.principal in contained);
      } else {
        send l.pdp, eLookupResult, (ok = false, version = 0, contained = false);
      }
    }

    on eSetStoreUp do (u: bool) { up = u; }

    on eSetLink do (l: (replica: machine, up: bool)) {
      if (l.up) {
        if (l.replica in cut) {
          cut -= (l.replica);
          send l.replica, eRelisten;
        }
      } else {
        cut += (l.replica);
      }
    }
  }
}
