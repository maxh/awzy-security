// A gateway's outage fallback (ADR 0030, ADR 0089, ADR 0109): while the PDP answers, the
// gateway forwards what it allows and remembers each allow; while it doesn't, the gateway
// replays a recent allow for a tool classified as failing open, forwards everything if the
// install was monitoring, and denies the rest. Each tool's mode can be its own (ADR 0109).
//
// One caller, so a tool stands for the whole call: who, for whom, and what.

event eCall: (caller: machine, tool: int, now: int);
event eCalled;
// Authorizer.Check, and its answer: `up` is false when the PDP doesn't answer.
event eAsk: (gateway: machine, tool: int);
event eAnswer: (up: bool, allow: bool, monitor: bool);

event eSetPdpUp: bool;
// A publish or a mode switch: the tool's verdict and mode from now on.
event eSetPolicy: (tool: int, allow: bool, monitor: bool);

// Announced for the specs. The PDP's decision, and what the gateway did with the call: 0
// blocked, 1 forwarded on an allow, 2 forwarded in monitor mode, 3 forwarded in an outage as
// monitoring, 4 forwarded in an outage on a replayed allow, 5 denied in an outage.
event eDecided: (tool: int, allow: bool, monitor: bool, at: int);
event eOutcome: (tool: int, how: int, at: int);
