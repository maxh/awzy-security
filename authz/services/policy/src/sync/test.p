// awzy-policy and two PDPs, the second starting later. Admins publish bundles and switch modes,
// awzy-policy goes down and comes back, and in the tampered runs someone with Postgres access
// replays, deletes, and forges rows. The last few ticks are quiet, with awzy-policy up.

fun Rounds(): int { return 16; }
fun BusyRounds(): int { return 12; }

machine PolicyClock {
  var store: machine;
  var pdps: seq[machine];
  var now: int;
  var tamper: bool;
  var up: bool;

  start state Running {
    entry (t: bool) {
      var first: machine;
      tamper = t;
      up = true;
      store = new PolicyStore();
      first = new PolicyPdp((store = store, peer = default(machine), startAt = 0));
      pdps += (0, first);
      pdps += (1, new PolicyPdp((store = store, peer = first, startAt = choose(BusyRounds()) + 1)));
      while (now < Rounds()) {
        now = now + 1;
        if (now <= BusyRounds()) { act(); } else if (!up) { up = true; send store, eSetUp, true; }
        tick();
      }
      announce eEnd;
    }
  }

  fun act() {
    var what: int;
    what = choose(6);
    if (what == 0) {
      send store, ePublish;
    } else if (what == 1) {
      send store, eSetInstall, choose(2);
    } else if (what == 2) {
      send store, eSetScoped, choose(2);
    } else if (what == 3) {
      up = !up;
      send store, eSetUp, up;
    } else if (what == 4 && tamper) {
      send store, eTamper, choose(6);
    }
  }

  fun tick() {
    var m: machine;
    send store, eTick, (clock = this, now = now);
    receive { case eTickAck: {} }
    foreach (m in pdps) {
      send m, eTick, (clock = this, now = now);
      receive { case eTickAck: {} }
    }
  }
}

machine Honest {
  start state Run { entry { new PolicyClock(false); } }
}

machine Tampered {
  start state Run { entry { new PolicyClock(true); } }
}

test tcOnlySigned [main = Tampered]: assert OnlySigned in { Tampered, PolicyClock, PolicyStore, PolicyPdp };
test tcNeverBackwards [main = Tampered]: assert NeverBackwards in { Tampered, PolicyClock, PolicyStore, PolicyPdp };
test tcStartsCurrent [main = Tampered]: assert StartsCurrent in { Tampered, PolicyClock, PolicyStore, PolicyPdp };
test tcConverges [main = Honest]: assert Converges in { Honest, PolicyClock, PolicyStore, PolicyPdp };
