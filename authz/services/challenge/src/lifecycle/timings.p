// awzy-challenge's lifetimes, in ticks, scaled down so the checker reaches them.

// How long a challenge waits for an answer, and a rejection stands (AWZY_CHALLENGE_PENDING_SECS,
// 24 hours).
fun PendingLife(): int { return 6; }
// How long a passed challenge grants its request (AWZY_CHALLENGE_GRANT_SECS, 15 minutes).
fun GrantLife(): int { return 4; }

// The kinds a policy asks for: approval (0) or justification (1).
fun Approval(): int { return 0; }
fun Justification(): int { return 1; }
// Policy 1 asks for an approval; policies 2 and 3 each for a justification.
fun KindOf(policy: int): int {
  if (policy == 1) { return Approval(); }
  return Justification();
}
