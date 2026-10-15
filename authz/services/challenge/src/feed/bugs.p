// Bugs this model found, each fixed. CI puts each back in turn, by having Bug() return its
// number, and fails unless the named test then breaks, so the model keeps catching what it
// caught (tools/ci/check-p-models.sh). Bug() returns 0 everywhere else.
//
// bug 1 breaks tcNoRollback: a PDP or gateway took any list its stream sent, so reconnecting to
// a replica that hadn't reloaded rolled it back to an older list.
// bug 2 breaks tcBoundedStaleness: a replica vouched for its list without having read Postgres
// lately, so a replica cut off from it kept its PDPs fresh on an old list.
// bug 3 breaks tcBoundedStaleness: a list that went out while its replica was quiet counted as
// sent, and a heartbeat freshened whatever list the PDP held, not the version it named.
fun Bug(): int { return 0; }
