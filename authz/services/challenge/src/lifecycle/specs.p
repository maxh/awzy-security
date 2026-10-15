// What a challenge promises.

// A request is allowed on a grant only for the policies that grant answered: each policy that
// asks was on the challenge when its kind was passed, and the grant is live. Nothing is allowed
// while a rejection stands.
spec AllowsOnlyWhatWasAnswered observes eOpened, eMerged, eKindPassed, eGranted, eRejected, eDecision {
  var policies: map[int, tPolicies];
  // Per challenge, the policies whose kind was passed while they were on it.
  var answered: map[int, tPolicies];
  var grantUntil: map[int, int];
  var rejectedUntil: int;

  start state Watching {
    on eOpened do (o: (challenge: int, policies: tPolicies)) {
      policies[o.challenge] = o.policies;
      answered[o.challenge] = default(tPolicies);
    }

    on eMerged do (m: (challenge: int, policies: tPolicies)) {
      var p: int;
      foreach (p in m.policies) { policies[m.challenge] += (p); }
    }

    on eKindPassed do (k: (challenge: int, kind: int, at: int)) {
      var p: int;
      foreach (p in policies[k.challenge]) {
        if (KindOf(p) == k.kind) { answered[k.challenge] += (p); }
      }
    }

    on eGranted do (g: (challenge: int, until: int)) { grantUntil[g.challenge] = g.until; }

    on eRejected do (r: (challenge: int, until: int)) { rejectedUntil = r.until; }

    on eDecision do (d: (asking: tPolicies, answer: int, at: int)) {
      var p: int;
      var c: int;
      var ok: bool;
      if (d.answer != 1) { return; }
      assert d.at >= rejectedUntil, format("allowed at {0} while a rejection stands until {1}", d.at, rejectedUntil);
      foreach (p in d.asking) {
        ok = false;
        foreach (c in keys(grantUntil)) {
          if (grantUntil[c] > d.at && p in answered[c]) { ok = true; }
        }
        assert ok, format("allowed at {0}, but no live grant answered policy {1}: {2} answered {3}",
          d.at, p, policies, answered);
      }
    }
  }
}

// Retries never pile up challenges: at most one is pending at a time.
spec OnePending observes eOpened, eGranted, eRejected, eExpired {
  var pending: map[int, int];

  start state Watching {
    on eOpened do (o: (challenge: int, policies: tPolicies)) {
      var c: int;
      foreach (c in keys(pending)) {
        assert false, format("challenge {0} opened while challenge {1} is still pending", o.challenge, c);
      }
      pending[o.challenge] = 0;
    }
    on eGranted do (g: (challenge: int, until: int)) { pending -= (g.challenge); }
    on eRejected do (r: (challenge: int, until: int)) { pending -= (r.challenge); }
    on eExpired do (c: int) { pending -= (c); }
  }
}
