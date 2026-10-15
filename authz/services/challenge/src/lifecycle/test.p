// One request, checked every tick by a PDP. The bundle in force changes which policies ask; the
// person justifies, and an approver approves or rejects, whenever they like.

fun Rounds(): int { return 14; }

machine Requester {
  var store: machine;
  var pdp: machine;
  var now: int;
  var asking: tPolicies;

  start state Running {
    entry {
      store = new ChallengeStore();
      pdp = new ChallengePdp(store);
      asking = bundle(1);
      while (now < Rounds()) {
        now = now + 1;
        send store, eTick, (clock = this, now = now);
        receive { case eTickAck: {} }
        send pdp, eTick, (clock = this, now = now);
        receive { case eTickAck: {} }
        act();
        send pdp, eCheck, (clock = this, asking = asking, now = now);
        receive { case eTickAck: {} }
      }
    }
  }

  fun act() {
    var what: int;
    what = choose(5);
    if (what == 0) {
      send store, eAnswer, (kind = Justification(), approve = true);
    } else if (what == 1) {
      send store, eAnswer, (kind = Approval(), approve = true);
    } else if (what == 2) {
      send store, eAnswer, (kind = Approval(), approve = false);
    } else if (what == 3) {
      asking = bundle(choose(4));
    }
  }

  // The policies that ask in each bundle a publish might bring.
  fun bundle(n: int): tPolicies {
    var b: tPolicies;
    if (n == 0 || n == 1 || n == 3) { b += (1); }
    if (n == 1 || n == 2) { b += (2); }
    if (n >= 1) { b += (3); }
    return b;
  }
}

test tcAllowsOnlyWhatWasAnswered [main = Requester]:
  assert AllowsOnlyWhatWasAnswered in { Requester, ChallengeStore, ChallengePdp };
test tcOnePending [main = Requester]:
  assert OnePending in { Requester, ChallengeStore, ChallengePdp };
