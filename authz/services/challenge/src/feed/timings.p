// The PDP's and awzy-challenge's timings, in ticks of a second. The long ones are scaled down
// so the checker reaches them; what matters is how they compare.

// No heartbeat for this long makes a PDP's list stale (STALE_AFTER, 2 s).
fun StaleAfter(): int { return 2; }
// How long a stale list stands in while awzy-challenge can't be asked
// (AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS, 300 s).
fun MaxStale(): int { return 6; }
// After a failed lookup, checks use the last list without asking for this long
// (PAUSE_AFTER_FAILED_LOOKUP, 1 s).
fun LookupPause(): int { return 1; }
// A replica reloads this often without a notification (RELOAD_EVERY, 5 s).
fun ReloadEvery(): int { return 3; }
// A replica vouches for its list on its streams only this long after it last read Postgres
// (CONFIRMED_FOR, 10 s): two reloads.
fun ConfirmedFor(): int { return 2 * ReloadEvery(); }
// A follower reconnects this long after its stream breaks (1 s).
fun ReconnectAfter(): int { return 1; }
