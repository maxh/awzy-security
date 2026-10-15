// awzy-policy's store (store.rs) as its gRPC service serves it (grpc.rs): bundles with one
// active, and the append-only history of mode switches, each signed when it's made. The latest
// switch of a scope is its mode; with none, the scope is monitor, unsigned.
machine PolicyStore {
  var now: int;
  var up: bool;
  var bundles: seq[tBundle];
  var active: int;
  var installs: seq[tSetting];
  var scopeds: seq[tSetting];
  var made_: tState;

  start state Serving {
    entry {
      up = true;
      active = -1;
      made_.install = (at = -1, mode = 0, genuine = false);
      made_.scoped = (at = -1, mode = 0, genuine = false);
    }

    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      send t.clock, eTickAck;
    }

    on eSetUp do (u: bool) { up = u; }

    on eGetState do (pdp: machine) {
      if (up) {
        send pdp, eStateAnswer, (ok = true, policy = current());
      } else {
        send pdp, eStateAnswer, (ok = false, policy = current());
      }
    }

    on ePublish do {
      bundles += (sizeof(bundles), (version = sizeof(bundles) + 1, genuine = true));
      active = sizeof(bundles) - 1;
      made();
    }

    on eSetInstall do (mode: int) {
      installs += (sizeof(installs), (at = now, mode = mode, genuine = true));
      made();
    }

    on eSetScoped do (mode: int) {
      scopeds += (sizeof(scopeds), (at = now, mode = mode, genuine = true));
      made();
    }

    on eTamper do (how: int) {
      if (how == 0 && active > 0) {
        active = choose(active);
      } else if (how == 1) {
        bundles += (sizeof(bundles), (version = sizeof(bundles) + 1, genuine = false));
        active = sizeof(bundles) - 1;
      } else if (how == 2) {
        installs = default(seq[tSetting]);
      } else if (how == 3) {
        scopeds = default(seq[tSetting]);
      } else if (how == 4) {
        installs += (sizeof(installs), (at = now, mode = 0, genuine = false));
      } else if (how == 5 && sizeof(installs) > 0) {
        installs -= (sizeof(installs) - 1);
      }
    }
  }

  fun current(): tState {
    var s: tState;
    if (active >= 0) { s.bundle = bundles[active]; }
    if (sizeof(installs) > 0) {
      s.install = installs[sizeof(installs) - 1];
    } else {
      s.install = (at = -1, mode = 0, genuine = false);
    }
    if (sizeof(scopeds) > 0) {
      s.scoped = scopeds[sizeof(scopeds) - 1];
      s.hasScoped = true;
    }
    return s;
  }

  // What awzy-policy itself made, newest of each, for the specs: what tampering can't change.
  fun made() {
    var b: tBundle;
    var s: tState;
    var i: int;
    i = sizeof(bundles) - 1;
    while (i >= 0) {
      if (bundles[i].genuine && b.version == 0) { b = bundles[i]; }
      i = i - 1;
    }
    if (b.version > made_.bundle.version) { made_.bundle = b; }
    if (sizeof(installs) > 0 && installs[sizeof(installs) - 1].genuine) { made_.install = installs[sizeof(installs) - 1]; }
    if (sizeof(scopeds) > 0 && scopeds[sizeof(scopeds) - 1].genuine) {
      made_.scoped = scopeds[sizeof(scopeds) - 1];
      made_.hasScoped = true;
    }
    announce eMade, made_;
  }
}
