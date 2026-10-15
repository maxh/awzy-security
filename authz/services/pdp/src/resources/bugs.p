// Bugs this model found, each fixed. CI puts each back in turn, by having Bug() return its
// number, and fails unless the named test then breaks, so the model keeps catching what it
// caught (tools/ci/check-p-models.sh). Bug() returns 0 everywhere else.
//
// bug 1 breaks tcChangesReachChecks: the change log's cursor was the newest row committed, so a
// write that committed after a later-numbered one fell behind it and was never named.
// bug 2 breaks tcChangesReachChecks: the follower's first answer only set the cursor, so what
// was cached before it stayed for the TTL.
//
// Not here: a fetch in flight cached after a drop (the epoch in ResourceCache). The checker
// didn't reach it in 10,000 schedules without the guard, so it's left to the Rust test.
fun Bug(): int { return 0; }
