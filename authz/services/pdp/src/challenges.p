// A PDP deciding a challenged request (challenges.rs, and resolve_challenge in lib.rs): it asks
// awzy-challenge to resolve the policies that ask, denies while a rejection stands, sets aside the
// policies a grant covers, and allows once none is left.
machine ChallengePdp {
  var store: machine;

  start state Init {
    entry (s: machine) {
      store = s;
      goto Deciding;
    }
  }

  state Deciding {
    on eTick do (t: (clock: machine, now: int)) {
      send t.clock, eTickAck;
    }

    on eCheck do (c: (clock: machine, asking: tPolicies, now: int)) {
      var answer: int;
      var p: int;
      var left: bool;
      send store, eResolve, (pdp = this, asking = c.asking);
      receive {
        case eResolution: (r: (covered: tPolicies, rejected: bool, challenge: int)) {
          if (r.rejected) {
            answer = 0;
          } else {
            foreach (p in c.asking) {
              if (!(p in r.covered)) { left = true; }
            }
            if (left) { answer = 2; } else { answer = 1; }
          }
        }
      }
      announce eDecision, (asking = c.asking, answer = answer, at = c.now);
      send c.clock, eTickAck;
    }
  }
}
