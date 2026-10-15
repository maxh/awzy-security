// A PDP's containments (containments.rs) and how a starting one takes a running one's list
// (peer.rs). It follows one replica's WatchContainments stream and reconnects to any replica
// after the stream breaks.
machine Pdp {
  var store: machine;
  var peer: machine;
  var replicas: seq[machine];
  var pausedUntil: int;
  var now: int;
  var started: bool;
  var startAt: int;
  var stream: int;
  var connected: bool;
  var replica: machine;
  var reconnectAt: int;
  var loaded: bool;
  var version: int;
  var contained: tContained;
  var heardAt: int;

  start state Init {
    entry (c: (store: machine, peer: machine, replicas: seq[machine], startAt: int)) {
      store = c.store;
      peer = c.peer;
      replicas = c.replicas;
      startAt = c.startAt;
      goto Following;
    }
  }

  state Following {
    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      if (!started && now >= startAt) {
        started = true;
        announce eFollowerStarted, this;
      }
      if (started && !connected && now >= reconnectAt) { connect(); }
      // A PDP that isn't ready asks a running one (ADR 0119).
      if (started && !loaded && peer != null && $) { startFromPeer(); }
      send t.clock, eTickAck;
    }

    on eGetState do (from: machine) {
      send from, eState, (loaded = loaded, version = version, contained = contained, age = now - heardAt);
    }

    on eCheck do (c: (principal: int, now: int)) {
      now = c.now;
      check(c.principal);
    }

    on eFeedList do (l: (stream: int, version: int, contained: tContained)) {
      // Never an older list than the one held: the replica may lag the one sent it before.
      if (connected && l.stream == stream && (!loaded || l.version >= version || Bug() == 1)) {
        load(l.version, l.contained, now);
      }
    }

    on eHeartbeat do (h: (stream: int, version: int)) {
      // A heartbeat vouches only for the version it names.
      if (connected && h.stream == stream && (h.version == version || Bug() == 3)) { heardAt = now; }
    }

    on eStreamClosed do (s: int) {
      if (connected && s == stream) { disconnected(); }
    }

    on eBreakStream do {
      if (connected) {
        send replica, eUnsubscribe, (follower = this, stream = stream);
        disconnected();
      }
    }
  }

  fun startFromPeer() {
    send peer, eGetState, this;
    receive {
      case eState: (s: (loaded: bool, version: int, contained: tContained, age: int)) {
        if (s.loaded && (!loaded || s.version > version)) { load(s.version, s.contained, now - s.age); }
      }
    }
  }

  // Containments::find: the list while the feed is fresh; while it's stale, a direct lookup;
  // when that fails, the last list up to MaxStale after the feed was last heard; then unavailable.
  fun check(principal: int) {
    var answer: int;
    var basis: int;
    var asked: bool;
    if (!loaded) {
      decide(principal, 2, 0);
      return;
    }
    if (now - heardAt <= StaleAfter()) {
      decide(principal, fromList(principal), version);
      return;
    }
    if (now >= pausedUntil) {
      send store, eLookup, (pdp = this, replica = choose(replicas), principal = principal);
      receive {
        case eLookupResult: (r: (ok: bool, version: int, contained: bool)) {
          if (r.ok) {
            asked = true;
            if (r.contained) { answer = 1; } else { answer = 0; }
            basis = r.version;
          } else {
            pausedUntil = now + LookupPause();
          }
        }
      }
      if (asked) {
        decide(principal, answer, basis);
        return;
      }
    }
    if (now - heardAt <= MaxStale()) {
      decide(principal, fromList(principal), version);
      return;
    }
    decide(principal, 2, version);
  }

  fun fromList(principal: int): int {
    if (principal in contained) { return 1; }
    return 0;
  }

  fun decide(principal: int, answer: int, basis: int) {
    announce eDecision, (pdp = this, principal = principal, answer = answer, basis = basis, at = now);
  }

  fun connect() {
    stream = stream + 1;
    replica = choose(replicas);
    connected = true;
    send replica, eSubscribe, (follower = this, stream = stream);
  }

  fun disconnected() {
    connected = false;
    reconnectAt = now + ReconnectAfter();
  }

  fun load(v: int, c: tContained, heard: int) {
    loaded = true;
    version = v;
    contained = c;
    heardAt = heard;
    announce eApplied, (follower = this, version = v);
  }
}
