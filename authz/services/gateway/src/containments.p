// The gateway's containments (containments.rs), for its outage fallback: it follows one
// replica's WatchContainments stream, reconnects to any replica after it breaks, and keeps the
// last list with no limit (ADR 0030 accepts that).
machine Gateway {
  var replicas: seq[machine];
  var now: int;
  var stream: int;
  var connected: bool;
  var replica: machine;
  var reconnectAt: int;
  var version: int;
  var contained: tContained;

  start state Init {
    entry (r: seq[machine]) {
      replicas = r;
      announce eFollowerStarted, this;
      goto Following;
    }
  }

  state Following {
    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      if (!connected && now >= reconnectAt) { connect(); }
      send t.clock, eTickAck;
    }

    on eFeedList do (l: (stream: int, version: int, contained: tContained)) {
      // Never an older list than the one held: the replica may lag the one sent it before.
      if (connected && l.stream == stream && (l.version >= version || Bug() == 1)) {
        version = l.version;
        contained = l.contained;
        announce eApplied, (follower = this, version = l.version);
      }
    }

    on eHeartbeat do (h: (stream: int, version: int)) {}

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
}
