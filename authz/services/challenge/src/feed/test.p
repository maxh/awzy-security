// Two awzy-challenge replicas on one Postgres, two PDPs (the second starting later, from its
// peer when it can), and a gateway. For the first ChaosRounds() ticks, people contain and lift
// principals while Postgres goes down, replicas lose it or restart, and streams break; then
// everything heals. A PDP is asked about a principal every tick.

fun Rounds(): int { return 26; }
fun ChaosRounds(): int { return 16; }
fun Principals(): int { return 2; }

machine Clock {
  var now: int;
  var store: machine;
  var replicas: seq[machine];
  var pdps: seq[machine];
  var gateway: machine;
  var everyone: seq[machine];
  var storeUp: bool;
  var cut: set[machine];

  start state Running {
    entry {
      var first: machine;
      store = new Store();
      replicas += (0, new Replica(store));
      replicas += (1, new Replica(store));
      send store, eReplicas, replicas;
      first = new Pdp((store = store, peer = default(machine), replicas = replicas, startAt = 0));
      pdps += (0, first);
      pdps += (1, new Pdp((store = store, peer = first, replicas = replicas, startAt = choose(ChaosRounds()) + 1)));
      gateway = new Gateway(replicas);
      everyone += (0, store);
      everyone += (1, replicas[0]);
      everyone += (2, replicas[1]);
      everyone += (3, pdps[0]);
      everyone += (4, pdps[1]);
      everyone += (5, gateway);
      storeUp = true;
      while (now < Rounds()) {
        now = now + 1;
        tick();
        if (now <= ChaosRounds()) {
          if ($) { send store, eWrite, (principal = choose(Principals()), contain = $); }
          disturb();
        } else if (now == ChaosRounds() + 1) {
          heal();
        }
        send choose(pdps), eCheck, (principal = choose(Principals()), now = now);
      }
      tick();
      announce eEnd, now;
    }
  }

  fun tick() {
    var m: machine;
    var acks: int;
    foreach (m in everyone) {
      send m, eTick, (clock = this, now = now);
    }
    while (acks < sizeof(everyone)) {
      receive {
        case eTickAck: { acks = acks + 1; }
      }
    }
  }

  fun disturb() {
    var r: machine;
    var what: int;
    what = choose(6);
    if (what == 0) {
      storeUp = !storeUp;
      send store, eSetStoreUp, storeUp;
    } else if (what == 1) {
      r = choose(replicas);
      if (r in cut) { cut -= (r); } else { cut += (r); }
      send store, eSetLink, (replica = r, up = !(r in cut));
    } else if (what == 2) {
      send choose(replicas), eRestart;
    } else if (what == 3) {
      send choose(pdps), eBreakStream;
    } else if (what == 4) {
      send gateway, eBreakStream;
    }
  }

  fun heal() {
    var r: machine;
    storeUp = true;
    send store, eSetStoreUp, true;
    foreach (r in cut) {
      send store, eSetLink, (replica = r, up = true);
    }
    cut = default(set[machine]);
  }
}

// One test per property, so each reports its own counterexample.
test tcBoundedStaleness [main = Clock]:
  assert BoundedStaleness in { Clock, Store, Replica, Pdp, Gateway };
test tcNoRollback [main = Clock]:
  assert NoRollback in { Clock, Store, Replica, Pdp, Gateway };
test tcConverges [main = Clock]:
  assert Converges in { Clock, Store, Replica, Pdp, Gateway };
