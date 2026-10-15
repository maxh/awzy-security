// What the containment feed promises.

// The longest a PDP may decide "not contained" from a list after Postgres moved past it: how
// long a replica that lost Postgres keeps vouching for its list, the time its last word takes to
// arrive (the PDP counts staleness from then), and the PDP's own limit on a stale one (ADR 0129).
fun Bound(): int { return ConfirmedFor() + Delivery() + MaxStale(); }

// A PDP never answers "not contained" for a principal contained in the list it answered from,
// nor from a list Postgres replaced more than Bound() ago. Failing as unavailable is always
// allowed.
spec BoundedStaleness observes eStoreChanged, eDecision {
  var lists: map[int, tContained];
  var replacedAt: map[int, int];
  var current: int;

  start state Watching {
    on eStoreChanged do (c: (version: int, contained: tContained, at: int)) {
      if (current > 0) { replacedAt[current] = c.at; }
      lists[c.version] = c.contained;
      current = c.version;
    }

    on eDecision do (d: (pdp: machine, principal: int, answer: int, basis: int, at: int)) {
      if (d.answer != 0) { return; }
      assert d.basis in lists, format("{0} answered from version {1}, which Postgres never had", d.pdp, d.basis);
      assert !(d.principal in lists[d.basis]),
        format("{0} said principal {1} isn't contained, from version {2}, which contains them", d.pdp, d.principal, d.basis);
      if (d.basis != current) {
        assert d.at - replacedAt[d.basis] <= Bound(),
          format("at {0}, {1} said principal {2} isn't contained from version {3}, replaced at {4} by version {5}",
            d.at, d.pdp, d.principal, d.basis, replacedAt[d.basis], current);
      }
    }
  }
}

// A PDP or gateway never goes back to an older list than one it has applied, so a containment
// it has enforced isn't forgotten until it's lifted.
spec NoRollback observes eApplied {
  var applied: map[machine, int];

  start state Watching {
    on eApplied do (a: (follower: machine, version: int)) {
      if (a.follower in applied) {
        assert a.version >= applied[a.follower],
          format("{0} went back from version {1} to version {2}", a.follower, applied[a.follower], a.version);
      }
      applied[a.follower] = a.version;
    }
  }
}

// Once the failures stop, every follower has Postgres's latest list by the end of the run.
spec Converges observes eStoreChanged, eApplied, eFollowerStarted, eEnd {
  var applied: map[machine, int];
  var current: int;

  start state Watching {
    on eStoreChanged do (c: (version: int, contained: tContained, at: int)) { current = c.version; }
    on eFollowerStarted do (f: machine) { applied[f] = 0; }
    on eApplied do (a: (follower: machine, version: int)) { applied[a.follower] = a.version; }
    on eEnd do (at: int) {
      var f: machine;
      foreach (f in keys(applied)) {
        assert applied[f] == current,
          format("at {0}, {1} still has version {2} of {3}", at, f, applied[f], current);
      }
    }
  }
}
