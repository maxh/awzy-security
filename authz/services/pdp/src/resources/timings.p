// The PDP's resource cache timings, in ticks.

// How long a fetched resource is used while the change log is heard from (FEED_TTL, 300 s).
fun FeedTtl(): int { return 8; }
// How long after a change every PDP must decide on it (the plan's 60-second target).
fun ChangeTarget(): int { return 3; }
