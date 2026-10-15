// Bugs this model found, each fixed. CI puts each back in turn, by having Bug() return its
// number, and fails unless the named test then breaks, so the model keeps catching what it
// caught (tools/ci/check-p-models.sh). Bug() returns 0 everywhere else.
//
// bug 1 breaks tcAllowsOnlyWhatWasAnswered: a policy that joined a pending challenge kept the
// kinds already passed, so one approval granted a justification no one gave.
fun Bug(): int { return 0; }
