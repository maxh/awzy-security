// Bugs this model found, each fixed. CI puts each back in turn, by having Bug() return its
// number, and fails unless the named test then breaks, so the model keeps catching what it
// caught (tools/ci/check-p-models.sh). Bug() returns 0 everywhere else.
//
// bug 1 breaks tcOutagesKeepEachToolsMode: the last answer's mode was one flag for the whole
// gateway, so one tool in monitor made an outage forward every tool.
// bug 2 breaks tcReplaysOnlyTheLatestAllow: a deny didn't clear the cached allow, so an outage
// replayed an allow a later decision overruled.
fun Bug(): int { return 0; }
