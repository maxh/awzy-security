// Bugs this model found, each fixed. CI puts each back in turn, by having Bug() return its
// number, and fails unless the named test then breaks, so the model keeps catching what it
// caught (tools/ci/check-p-models.sh). Bug() returns 0 everywhere else.
//
// bug 1 breaks tcNeverBackwards: a scope left out of the list dropped its mode, so deleting its
// switches put every running PDP back on the install's mode.
// bug 2 breaks tcStartsCurrent: a starting PDP took awzy-policy's answer without asking its
// peers first, so deleting the install's switches started it in monitor.
fun Bug(): int { return 0; }
