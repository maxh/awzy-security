// The containment feed (ADR 0054, ADR 0119): awzy-challenge's replicas read the active
// containments from Postgres and stream them to every PDP and gateway, which decide from the
// last list they were sent.
//
// Time is in ticks of a second. A tick goes to every machine and each acknowledges it before the
// next, so a message sent while handling a tick arrives before the next one, and one sent in reply
// to another message may arrive in the next: the model assumes the network is faster than a
// second, and looks at everything else that can go wrong.

// Principals are numbered from 0.
type tContained = set[int];

// The clock, to every machine.
event eTick: (clock: machine, now: int);
event eTickAck;

// Postgres.
event eWrite: (principal: int, contain: bool);
// A replica's read of the version and the active list, in one snapshot. `ok` is false when the
// database, or the replica's link to it, is down.
event eRead: machine;
event eReadResult: (ok: bool, version: int, contained: tContained, at: int);
// pg_notify of a new version, to each replica whose listener is connected.
event eNotify: int;
// A replica's link to Postgres came back: its listener reconnects and reloads.
event eRelisten;
// A PDP's direct lookup while its feed is stale (ListContainments), through `replica`.
event eLookup: (pdp: machine, replica: machine, principal: int);
event eLookupResult: (ok: bool, version: int, contained: bool);

// What the environment does to the system.
event eSetStoreUp: bool;
event eSetLink: (replica: machine, up: bool);
event eRestart;
event eBreakStream;
event eCheck: (principal: int, now: int);

// WatchContainments, one stream from a replica to a PDP or gateway.
event eSubscribe: (follower: machine, stream: int);
event eUnsubscribe: (follower: machine, stream: int);
event eFeedList: (stream: int, version: int, contained: tContained);
event eHeartbeat: (stream: int, version: int);
event eStreamClosed: int;

// PdpPeerService.GetState, for a PDP that's starting.
event eGetState: machine;
event eState: (loaded: bool, version: int, contained: tContained, age: int);

// Announced for the specs.
event eStoreChanged: (version: int, contained: tContained, at: int);
event eApplied: (follower: machine, version: int);
event eFollowerStarted: machine;
// 0: not contained, 1: contained, 2: unavailable. `basis` is the list version the answer came
// from.
event eDecision: (pdp: machine, principal: int, answer: int, basis: int, at: int);
event eCalm: int;
event eEnd: int;

// Who a store notifies.
event eReplicas: seq[machine];
