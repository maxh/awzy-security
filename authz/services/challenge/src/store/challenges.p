// The challenges half of awzy-challenge's store (store.rs): Resolve, the answers, and expiry, in
// Postgres. Each answer is one transaction on the locked row; Resolve's reads and writes are two
// steps another transaction can commit between.

type tChallenge = (policies: tPolicies, kinds: tKinds, passed: tKinds, status: int, expiresAt: int, grantUntil: int);

machine ChallengeStore {
  var now: int;
  // Indexed by challenge ID, from 0. Status: 0 pending, 1 passed, 2 rejected, 3 expired.
  var challenges: seq[tChallenge];

  start state Serving {
    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      send t.clock, eTickAck;
    }

    on eResolve do (r: (pdp: machine, asking: tPolicies)) {
      var i: int;
      var p: int;
      var covered: tPolicies;
      var rejected: bool;
      while (i < sizeof(challenges)) {
        if (challenges[i].status == 1 && challenges[i].grantUntil > now) {
          foreach (p in r.asking) {
            if (KindOf(p) in challenges[i].passed && p in challenges[i].policies) { covered += (p); }
          }
        }
        if (challenges[i].status == 2 && challenges[i].expiresAt > now) { rejected = true; }
        i = i + 1;
      }
      send this, eResolveWrite, (pdp = r.pdp, asking = r.asking, covered = covered, rejected = rejected);
    }

    on eResolveWrite do (r: (pdp: machine, asking: tPolicies, covered: tPolicies, rejected: bool)) {
      var i: int;
      var p: int;
      var id: int;
      var unanswered: tPolicies;
      var kinds: tKinds;
      var c: tChallenge;
      id = -1;
      foreach (p in r.asking) {
        if (!(p in r.covered)) {
          unanswered += (p);
          kinds += (KindOf(p));
        }
      }
      if (!r.rejected && sizeof(unanswered) > 0) {
        // Pending challenges past their time are marked expired, then the live one, if any,
        // takes the policies left (ON CONFLICT ... DO UPDATE); otherwise one is opened.
        while (i < sizeof(challenges)) {
          if (challenges[i].status == 0 && challenges[i].expiresAt <= now) {
            challenges[i].status = 3;
            announce eExpired, i;
          }
          if (challenges[i].status == 0) { id = i; }
          i = i + 1;
        }
        if (id >= 0) {
          // A policy new to the challenge needs its kind answered while it's on it: an answer
          // given before it asked isn't one (ADR 0080).
          c = challenges[id];
          foreach (p in unanswered) {
            if (!(p in c.policies)) { c.passed -= (KindOf(p)); }
            c.policies += (p);
            c.kinds += (KindOf(p));
          }
          challenges[id] = c;
          announce eMerged, (challenge = id, policies = unanswered);
        } else {
          id = sizeof(challenges);
          challenges += (id, (policies = unanswered, kinds = kinds, passed = default(tKinds), status = 0,
            expiresAt = now + PendingLife(), grantUntil = 0));
          announce eOpened, (challenge = id, policies = unanswered);
        }
      }
      send r.pdp, eResolution, (covered = r.covered, rejected = r.rejected, challenge = id);
    }

    // justify and decide: lock the pending challenge, check it can take this answer
    // (ensure_answerable, with a pending one past its time reading as expired), then mark it.
    on eAnswer do (a: (kind: int, approve: bool)) {
      var i: int;
      var id: int;
      var c: tChallenge;
      id = -1;
      while (i < sizeof(challenges)) {
        if (challenges[i].status == 0 && challenges[i].expiresAt > now) { id = i; }
        i = i + 1;
      }
      if (id < 0) { return; }
      c = challenges[id];
      if (!(a.kind in c.kinds) || a.kind in c.passed) { return; }
      if (a.kind == Approval() && !a.approve) {
        c.status = 2;
        c.expiresAt = now + PendingLife();
        challenges[id] = c;
        announce eRejected, (challenge = id, until = c.expiresAt);
        return;
      }
      c.passed += (a.kind);
      announce eKindPassed, (challenge = id, kind = a.kind, at = now);
      if (sizeof(c.passed) == sizeof(c.kinds)) {
        c.status = 1;
        c.grantUntil = now + GrantLife();
        announce eGranted, (challenge = id, until = c.grantUntil);
      }
      challenges[id] = c;
    }
  }
}
