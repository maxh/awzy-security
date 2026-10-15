// An awzy-challenge replica's feed (feed.rs) and its WatchContainments streams (grpc.rs).
machine Replica {
  var store: machine;
  var now: int;
  var loaded: bool;
  var version: int;
  var contained: tContained;
  var lastReload: int;
  // When the list was last read from Postgres, newer or not.
  var confirmedAt: int;
  // Each follower's open stream, and the version last sent on it.
  var streams: map[machine, int];
  var sentOn: map[machine, int];

  start state Init {
    entry (s: machine) {
      store = s;
      reload();
      goto Serving;
    }
  }

  state Serving {
    on eTick do (t: (clock: machine, now: int)) {
      var f: machine;
      now = t.now;
      if (now - lastReload >= ReloadEvery()) { reload(); }
      // Between changes, a heartbeat on every stream, while the list is one Postgres confirmed.
      foreach (f in keys(streams)) { tell(f); }
      send t.clock, eTickAck;
    }

    on eNotify do (v: int) { reload(); }
    on eRelisten do { reload(); }

    on eReadResult do (r: (ok: bool, version: int, contained: tContained, at: int)) {
      var f: machine;
      if (!r.ok) { return; }
      if (r.at > confirmedAt) { confirmedAt = r.at; }
      // Only a newer version replaces the list: reads can come back out of order.
      if (loaded && r.version <= version) { return; }
      loaded = true;
      version = r.version;
      contained = r.contained;
      foreach (f in keys(streams)) { tell(f); }
    }

    on eSubscribe do (s: (follower: machine, stream: int)) {
      streams[s.follower] = s.stream;
      sentOn[s.follower] = 0;
      tell(s.follower);
    }

    on eUnsubscribe do (s: (follower: machine, stream: int)) {
      if (s.follower in streams && streams[s.follower] == s.stream) {
        streams -= (s.follower);
        sentOn -= (s.follower);
      }
    }

    // The pod is replaced: its streams close and the new one starts with nothing loaded.
    on eRestart do {
      var f: machine;
      foreach (f in keys(streams)) {
        send f, eStreamClosed, streams[f];
      }
      streams = default(map[machine, int]);
      sentOn = default(map[machine, int]);
      loaded = false;
      version = 0;
      contained = default(tContained);
      reload();
    }
  }

  // Whether the list may go out as current: loaded, and read from Postgres recently. A replica
  // cut off from Postgres goes quiet, so its followers go stale instead of trusting it.
  fun vouches(): bool {
    return loaded && now - confirmedAt <= ConfirmedFor();
  }

  // The list if this stream hasn't had this version, a heartbeat if it has; nothing while the
  // replica doesn't vouch for its list.
  fun tell(f: machine) {
    if (!vouches()) { return; }
    if (sentOn[f] != version) {
      sentOn[f] = version;
      send f, eFeedList, (stream = streams[f], version = version, contained = contained);
    } else {
      send f, eHeartbeat, (stream = streams[f], version = version);
    }
  }

  fun reload() {
    lastReload = now;
    send store, eRead, this;
  }
}
