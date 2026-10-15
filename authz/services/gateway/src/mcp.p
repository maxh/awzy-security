// The gateway deciding a tool call (check and outage in mcp.rs).
machine Gateway {
  var pdp: machine;
  // When the PDP last allowed each tool.
  var allowedAt: map[int, int];
  // The mode the last answer for each tool carried: a tool's mode can be its own.
  var lastSeenMonitor: map[int, bool];
  // The mode of the last answer for any tool, for bug 1 only.
  var anyMonitor: bool;

  start state Init {
    entry (p: machine) {
      pdp = p;
      goto Serving;
    }
  }

  state Serving {
    on eCall do (c: (caller: machine, tool: int, now: int)) {
      var a: (up: bool, allow: bool, monitor: bool);
      send pdp, eAsk, (gateway = this, tool = c.tool);
      receive {
        case eAnswer: (r: (up: bool, allow: bool, monitor: bool)) { a = r; }
      }
      if (a.up) {
        lastSeenMonitor[c.tool] = a.monitor;
        anyMonitor = a.monitor;
        if (a.allow) {
          allowedAt[c.tool] = c.now;
          outcome(c.tool, 1, c.now);
        } else {
          // A later verdict overrules the allow, so an outage mustn't replay it.
          if (Bug() != 2) { allowedAt -= (c.tool); }
          if (a.monitor) { outcome(c.tool, 2, c.now); } else { outcome(c.tool, 0, c.now); }
        }
      } else if (seenMonitor(c.tool)) {
        outcome(c.tool, 3, c.now);
      } else if (FailsOpen(c.tool) && c.tool in allowedAt && c.now - allowedAt[c.tool] < AllowTtl()) {
        outcome(c.tool, 4, c.now);
      } else {
        outcome(c.tool, 5, c.now);
      }
      send c.caller, eCalled;
    }
  }

  fun seenMonitor(tool: int): bool {
    if (Bug() == 1) { return anyMonitor; }
    return tool in lastSeenMonitor && lastSeenMonitor[tool];
  }

  fun outcome(tool: int, how: int, at: int) {
    announce eOutcome, (tool = tool, how = how, at = at);
  }
}
