// The gateway's settings, in ticks.

// How long an allow can be replayed (cached_allow_ttl_secs, 5 minutes).
fun AllowTtl(): int { return 3; }
// Tool 0 is classified in a fail-open tier; tool 1 isn't, so it fails closed.
fun FailsOpen(tool: int): bool { return tool == 0; }
fun Tools(): int { return 2; }
