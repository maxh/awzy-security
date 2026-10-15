// The PDP as the gateway sees it: up or down, and per tool a verdict and a mode that publishes and
// switches change. A caller makes a call each tick.

fun Rounds(): int { return 12; }

machine OutagePdp {
  var up: bool;
  var allow: map[int, bool];
  var monitor: map[int, bool];
  var now: int;

  start state Serving {
    entry {
      var t: int;
      up = true;
      while (t < Tools()) {
        allow[t] = true;
        monitor[t] = false;
        t = t + 1;
      }
    }
    on eSetPdpUp do (u: bool) { up = u; }
    on eSetPolicy do (p: (tool: int, allow: bool, monitor: bool)) {
      allow[p.tool] = p.allow;
      monitor[p.tool] = p.monitor;
    }
    on eAsk do (a: (gateway: machine, tool: int)) {
      if (up) {
        announce eDecided, (tool = a.tool, allow = allow[a.tool], monitor = monitor[a.tool], at = now);
      }
      send a.gateway, eAnswer, (up = up, allow = allow[a.tool], monitor = monitor[a.tool]);
    }
    on eCall do (c: (caller: machine, tool: int, now: int)) { now = c.now; }
  }
}

machine Caller {
  var pdp: machine;
  var gateway: machine;
  var now: int;

  start state Calling {
    entry {
      var what: int;
      pdp = new OutagePdp();
      gateway = new Gateway(pdp);
      while (now < Rounds()) {
        now = now + 1;
        what = choose(3);
        if (what == 0) {
          send pdp, eSetPdpUp, $;
        } else if (what == 1) {
          send pdp, eSetPolicy, (tool = choose(Tools()), allow = $, monitor = $);
        }
        // The PDP learns the time first, so its decisions carry it.
        send pdp, eCall, (caller = this, tool = 0, now = now);
        send gateway, eCall, (caller = this, tool = choose(Tools()), now = now);
        receive { case eCalled: {} }
      }
    }
  }
}

test tcOutagesKeepEachToolsMode [main = Caller]:
  assert OutagesKeepEachToolsMode in { Caller, OutagePdp, Gateway };
test tcReplaysOnlyTheLatestAllow [main = Caller]:
  assert ReplaysOnlyTheLatestAllow in { Caller, OutagePdp, Gateway };
