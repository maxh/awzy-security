// What the outage fallback promises.

// An outage never forwards a call as monitoring unless that tool was last decided in monitor mode.
spec OutagesKeepEachToolsMode observes eDecided, eOutcome {
  var monitor: map[int, bool];

  start state Watching {
    on eDecided do (d: (tool: int, allow: bool, monitor: bool, at: int)) { monitor[d.tool] = d.monitor; }
    on eOutcome do (o: (tool: int, how: int, at: int)) {
      if (o.how != 3) { return; }
      assert o.tool in monitor && monitor[o.tool],
        format("at {0}, an outage forwarded a call to tool {1} as monitoring, though it was last decided in enforce mode", o.at, o.tool);
    }
  }
}

// An outage replays an allow only for a fail-open tool whose last decision was an allow, within
// the TTL.
spec ReplaysOnlyTheLatestAllow observes eDecided, eOutcome {
  var allowed: map[int, bool];
  var at: map[int, int];

  start state Watching {
    on eDecided do (d: (tool: int, allow: bool, monitor: bool, at: int)) {
      allowed[d.tool] = d.allow;
      at[d.tool] = d.at;
    }
    on eOutcome do (o: (tool: int, how: int, at: int)) {
      if (o.how != 4) { return; }
      assert FailsOpen(o.tool), format("replayed an allow for tool {0}, which fails closed", o.tool);
      assert allowed[o.tool], format("at {0}, replayed an allow for tool {1}, whose last decision was a deny", o.at, o.tool);
      assert o.at - at[o.tool] < AllowTtl(), format("at {0}, replayed an allow from {1}", o.at, at[o.tool]);
    }
  }
}
