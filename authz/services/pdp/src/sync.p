// A PDP's policy sync (sync.rs), and how a starting one takes policy from a running peer
// (peer.rs). Each tick it asks awzy-policy for the active bundle and the modes and applies them:
// a bundle only if newer and signed, the install's mode only if signed and no older, each scoped
// mode only if signed and no older. Any refusal ends that sync. Before its first sync, a PDP
// takes whatever newer signed policy its running peers hold, so a starting PDP can't be handed
// older policy than they enforce.
machine PolicyPdp {
  var store: machine;
  var peer: machine;
  var startAt: int;
  var now: int;
  var started: bool;
  // Ready once it has the install's mode (enforcement.is_ready()).
  var ready: bool;
  var held: tState;
  var askedPeers: bool;

  start state Init {
    entry (c: (store: machine, peer: machine, startAt: int)) {
      store = c.store;
      peer = c.peer;
      startAt = c.startAt;
      held.install = (at = -1, mode = 0, genuine = false);
      held.scoped = (at = -1, mode = 0, genuine = false);
      goto Syncing;
    }
  }

  state Syncing {
    on eTick do (t: (clock: machine, now: int)) {
      now = t.now;
      if (now >= startAt) { started = true; }
      if (started) { sync(); }
      send t.clock, eTickAck;
    }

    on ePeerState do (from: machine) {
      send from, ePeerStateAnswer, (ready = ready, policy = held);
    }
  }

  fun sync() {
    var answer: (ok: bool, policy: tState);
    if (!askedPeers && peer != null && Bug() != 2) {
      askedPeers = true;
      startFromPeer();
    }
    send store, eGetState, this;
    receive {
      case eStateAnswer: (a: (ok: bool, policy: tState)) { answer = a; }
    }
    if (answer.ok) {
      applyAll(answer.policy);
    } else if (!ready && peer != null) {
      startFromPeer();
    }
    announce eHeld, (pdp = this, ready = ready, policy = held);
  }

  // sync_waiting: the bundle, then the install's mode, then the scoped modes.
  fun applyAll(s: tState) {
    // GetActive answers `unchanged` for the version held.
    if (s.bundle.version != 0 && s.bundle.version != held.bundle.version) {
      if (!applyBundle(s.bundle)) { return; }
    }
    if (!applyInstall(s.install)) { return; }
    applyScoped(s.scoped, s.hasScoped);
  }

  fun startFromPeer() {
    send peer, ePeerState, this;
    receive {
      case ePeerStateAnswer: (a: (ready: bool, policy: tState)) {
        if (a.ready) {
          // apply_policy: the bundle, if the peer has one, then the modes, each checked as if
          // awzy-policy had sent them.
          if (a.policy.bundle.version != 0 && !applyBundle(a.policy.bundle)) { return; }
          if (!applyInstall(a.policy.install)) { return; }
          applyScoped(a.policy.scoped, a.policy.hasScoped);
        }
      }
    }
  }

  // apply_bundle: newer than the one in force, and signed.
  fun applyBundle(b: tBundle): bool {
    if (b.version <= held.bundle.version || !b.genuine) { return false; }
    held.bundle = b;
    return true;
  }

  // apply_setting and verify_setting.
  fun applyInstall(s: tSetting): bool {
    if (s.at == -1) {
      // A scope never switched: monitor, unsigned, and only while none signed was applied.
      if (s.mode != 0 || held.install.at != -1) { return false; }
      ready = true;
      return true;
    }
    if (!s.genuine || s.at < held.install.at) { return false; }
    held.install = s;
    ready = true;
    return true;
  }

  // apply_scoped: each scope listed replaces the one held if it's signed and no older. A scope
  // not listed keeps the mode held: switches are never deleted, so a missing one was.
  fun applyScoped(s: tSetting, has: bool) {
    if (!has) {
      if (Bug() == 1) {
        held.scoped = (at = -1, mode = 0, genuine = false);
        held.hasScoped = false;
      }
      return;
    }
    if (!s.genuine || (held.hasScoped && held.scoped.at > s.at)) { return; }
    held.scoped = s;
    held.hasScoped = true;
  }
}
