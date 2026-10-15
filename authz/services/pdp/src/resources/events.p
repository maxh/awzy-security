// A resource's attributes reaching the PDPs (ADR 0044, ADR 0077): awzy-context writes resources
// in Postgres, each write adding a row to the change log in its transaction, and a PDP caches
// what it fetched, following the change log to drop what changed.
//
// One resource. Its attributes are a version, moved by each write when it commits.

event eTick: (clock: machine, now: int);
event eTickAck;

// A connector's write: it takes a change-log row (its sequence number, or transaction ID) when it
// runs, and the write becomes visible when it commits.
event eBegin: int;
event eCommit: int;

// GET /v1/resources/{id}/entities.
event eFetch: machine;
event eFetched: int;
// GET /v1/resources/changes?after=: the next cursor, whether the resource is among the changes,
// and whether to drop everything.
event eChanges: (from: machine, after: int);
event eChangesAnswer: (cursor: int, changed: bool, reset: bool);

// The cache, shared by the checks and the change-log follower.
event eGet: machine;
event eEntry: (cached: bool, version: int, at: int);
event eStore: (version: int, at: int, epoch: int);
event eInvalidate;
event eEpoch: machine;
event eEpochIs: int;

event eCheck: (clock: machine, now: int);

// Announced for the specs.
event eCommitted: (version: int, at: int);
event eUsed: (version: int, at: int);
