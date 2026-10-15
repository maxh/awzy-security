// What signed policy promises, against someone who can write to awzy-policy's Postgres.

// A PDP holds only what awzy-policy signed.
spec OnlySigned observes eHeld {
  start state Watching {
    on eHeld do (h: (pdp: machine, ready: bool, policy: tState)) {
      assert h.policy.bundle.version == 0 || h.policy.bundle.genuine, format("{0} holds a forged bundle", h.pdp);
      assert h.policy.install.at == -1 || h.policy.install.genuine, format("{0} holds a forged install mode", h.pdp);
      assert !h.policy.hasScoped || h.policy.scoped.genuine, format("{0} holds a forged scoped mode", h.pdp);
    }
  }
}

// A PDP never goes back: not to an older bundle, an older mode, or a scope's mode before it was
// switched. Not to monitor from a signed mode either, by losing it.
spec NeverBackwards observes eHeld {
  var last: map[machine, tState];

  start state Watching {
    on eHeld do (h: (pdp: machine, ready: bool, policy: tState)) {
      var was: tState;
      if (h.pdp in last) {
        was = last[h.pdp];
        assert h.policy.bundle.version >= was.bundle.version,
          format("{0} went back from bundle {1} to {2}", h.pdp, was.bundle.version, h.policy.bundle.version);
        assert h.policy.install.at >= was.install.at,
          format("{0} went back to an install mode from {1}, after one from {2}", h.pdp, h.policy.install.at, was.install.at);
        assert !was.hasScoped || (h.policy.hasScoped && h.policy.scoped.at >= was.scoped.at),
          format("{0} lost or went back on its scoped mode: {1}, then {2}", h.pdp, was.scoped, h.policy.scoped);
      }
      last[h.pdp] = h.policy;
    }
  }
}

// A PDP that starts isn't behind the running ones: once ready, it holds no older policy than
// what a ready peer held when it last synced.
spec StartsCurrent observes eHeld {
  var last: map[machine, tState];
  var ready: map[machine, bool];

  start state Watching {
    on eHeld do (h: (pdp: machine, ready: bool, policy: tState)) {
      var other: machine;
      if (h.ready && !(h.pdp in ready && ready[h.pdp])) {
        foreach (other in keys(last)) {
          if (other != h.pdp && ready[other]) {
            assert h.policy.bundle.version >= last[other].bundle.version,
              format("{0} started on bundle {1} while {2} had {3}", h.pdp, h.policy.bundle.version, other, last[other].bundle.version);
            assert h.policy.install.at >= last[other].install.at,
              format("{0} started on the install mode from {1} while {2} had the one from {3}", h.pdp, h.policy.install.at, other, last[other].install.at);
            assert !last[other].hasScoped || (h.policy.hasScoped && h.policy.scoped.at >= last[other].scoped.at),
              format("{0} started without the scoped mode {1} had", h.pdp, other);
          }
        }
      }
      last[h.pdp] = h.policy;
      ready[h.pdp] = h.ready;
    }
  }
}

// With no one tampering, every PDP ends on what awzy-policy made last.
spec Converges observes eMade, eHeld, eEnd {
  var made: tState;
  var last: map[machine, tState];

  start state Watching {
    entry {
      made.install = (at = -1, mode = 0, genuine = false);
      made.scoped = (at = -1, mode = 0, genuine = false);
    }
    on eMade do (m: tState) { made = m; }
    on eHeld do (h: (pdp: machine, ready: bool, policy: tState)) { last[h.pdp] = h.policy; }
    on eEnd do {
      var p: machine;
      foreach (p in keys(last)) {
        assert last[p] == made, format("{0} ended on {1}, not {2}", p, last[p], made);
      }
    }
  }
}
