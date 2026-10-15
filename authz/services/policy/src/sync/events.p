// Signed policy reaching the PDPs (ADR 0021, ADR 0080, ADR 0109, ADR 0119): awzy-policy keeps
// the bundles and enforcement modes in Postgres, signs each when it's made, and every PDP polls
// for them, verifies them, and keeps the newest. A starting PDP that can't reach awzy-policy
// takes them from a running peer. Anyone who can write to awzy-policy's Postgres is in the
// model: they can't sign, but they can replay, delete, and forge rows.

// Version 0 is no bundle. `genuine` is whether awzy-policy signed it.
type tBundle = (version: int, genuine: bool);
// An enforcement mode: 0 monitor, 1 enforce. `at` is when it was switched, -1 for a scope never
// switched, which comes unsigned and is monitor.
type tSetting = (at: int, mode: int, genuine: bool);
// What awzy-policy, or a peer, answers: the active bundle, the install's mode, and the one scoped
// mode the model follows (a tool's), if it was ever switched.
type tState = (bundle: tBundle, install: tSetting, scoped: tSetting, hasScoped: bool);

event eTick: (clock: machine, now: int);
event eTickAck;

// GetActive, GetEnforcement, and ListEnforcementSettings, in one.
event eGetState: machine;
event eStateAnswer: (ok: bool, policy: tState);
// PdpPeerService.GetState.
event ePeerState: machine;
event ePeerStateAnswer: (ready: bool, policy: tState);

// Admins, through the client portal.
event ePublish;
event eSetInstall: int;
event eSetScoped: int;
// Someone who can write to Postgres. 0: the active bundle set back to an older one; 1: a forged
// bundle made active; 2: the install's mode switches deleted; 3: the scoped mode's switches
// deleted; 4: a forged install mode; 5: the newest install switch deleted, so an older one is
// latest.
event eTamper: int;
event eSetUp: bool;

// Announced for the specs.
event eMade: tState;
event eHeld: (pdp: machine, ready: bool, policy: tState);
event eEnd;
