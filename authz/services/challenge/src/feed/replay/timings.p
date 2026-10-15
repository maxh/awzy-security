// The real timings, in milliseconds, for checking a run's logs against specs.p (ADR 0130).
// `tools/ci/replay-p-model.py --timing MaxStale=60000` overrides one, for a run that sets
// AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS.

// CONFIRMED_FOR, 10 s.
fun ConfirmedFor(): int { return 10000; }
// How late a replica's last word may reach a PDP, plus how far apart two hosts' clocks may be,
// since the lines come from both.
fun Delivery(): int { return 1000; }
// AWZY_PDP_CONTAINMENTS_MAX_STALE_SECS, 300 s by default.
fun MaxStale(): int { return 300000; }
