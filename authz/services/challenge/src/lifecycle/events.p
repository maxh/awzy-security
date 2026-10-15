// A challenge's lifecycle (ADR 0054, ADR 0080): a PDP opens a challenge for a request its
// policies challenge, the person answers it or an approver decides it, and the PDP allows the
// request on a retry once a grant covers every policy that asks.
//
// One request, so every challenge here is for it. Policies are numbered; each asks for one kind.
// Time is in ticks that every machine acknowledges, as in the containment feed's model.

type tPolicies = set[int];
type tKinds = set[int];

event eTick: (clock: machine, now: int);
event eTickAck;

// ChallengeService.Resolve, from the PDP: which of `asking` a live grant covers, whether a
// rejection stands, and, with policies left, the pending challenge for them.
event eResolve: (pdp: machine, asking: tPolicies);
// The second half of Resolve's transaction, after its reads (READ COMMITTED: another
// transaction can commit in between).
event eResolveWrite: (pdp: machine, asking: tPolicies, covered: tPolicies, rejected: bool);
event eResolution: (covered: tPolicies, rejected: bool, challenge: int);

// Someone answers the pending challenge: the person justifies, an approver approves or rejects.
event eAnswer: (kind: int, approve: bool);

// A check of the request, with the policies that ask in the bundle in force.
event eCheck: (clock: machine, asking: tPolicies, now: int);

// Announced for the specs.
event eOpened: (challenge: int, policies: tPolicies);
event eMerged: (challenge: int, policies: tPolicies);
event eKindPassed: (challenge: int, kind: int, at: int);
event eGranted: (challenge: int, until: int);
event eRejected: (challenge: int, until: int);
event eExpired: int;
// 0: denied (rejected), 1: allowed, 2: challenged.
event eDecision: (asking: tPolicies, answer: int, at: int);
