// Package awzyauthz is awzy's Go client library: an enforcement point in your own service or
// agent (ADR 0045, 0114). It validates callers' tokens, asks awzy-pdp within a budget, applies
// the mode, surfaces challenges as a result to wait on, falls back as awzy-gateway does, and
// records activity. It's held to the conformance spec every awzy client library runs.
package awzyauthz

import "context"

// EntityRef is a typed reference to an entity: EntityRef{Type: "Human", ID: "alice"}.
type EntityRef struct {
	Type string `json:"type"`
	ID   string `json:"id"`
}

// Principal is who a check is about, from a validated token: the principal, and the person it
// acts for.
type Principal struct {
	Principal  EntityRef
	OnBehalfOf *EntityRef
	// HumanPresent says whether a person is there to answer a challenge; nil leaves it to the PDP.
	HumanPresent *bool
}

// Resource is what a call touches, with its tier when the app knows it: only a known tier can
// fail open.
type Resource struct {
	Type string
	ID   string
	Tier string
}

// Kind says which result a check gave.
type Kind string

const (
	KindAllowed         Kind = "allowed"
	KindDenied          Kind = "denied"
	KindChallenged      Kind = "challenged"
	KindUnavailable     Kind = "unavailable"
	KindUnauthenticated Kind = "unauthenticated"
)

// Result is what a check says to do. Allowed is true only for KindAllowed; the other fields are
// set as the kind says.
type Result struct {
	Allowed bool
	Kind    Kind
	// DecisionQID explains the decision later; empty when none was made.
	DecisionQID string
	// Mode and Verdict for an allowed call: in monitor mode, Verdict is what enforcing would do.
	Mode    string
	Verdict string
	// Fallback is "monitor" or "cached_allow" when the PDP couldn't be asked.
	Fallback string
	// Policies that denied it.
	Policies []string
	// ChallengeQID, Challenges, and ExpiresAt for a challenge the person must answer first.
	ChallengeQID string
	Challenges   []string
	ExpiresAt    string
	// Reason, for an unavailable or unauthenticated result.
	Reason string

	wait func(ctx context.Context) (ChallengeAnswer, error)
}

// Wait returns when the person answers the challenge, it expires, or ctx ends (pending). On
// "passed", retry the same call.
func (r Result) Wait(ctx context.Context) (ChallengeAnswer, error) {
	if r.wait == nil {
		return ChallengeAnswer{}, ErrNoChallenge
	}
	return r.wait(ctx)
}

// ChallengeAnswer is what became of a challenge: "passed" (retry), "rejected", "expired", or
// "pending".
type ChallengeAnswer struct {
	Outcome        string
	ExpiresAt      string
	GrantExpiresAt string
	Pending        []string
}

// DecisionNote is what the app's logs and traces get for each decision.
type DecisionNote struct {
	DecisionQID string
	Action      string
	Resource    EntityRef
	Principal   EntityRef
	OnBehalfOf  *EntityRef
	Result      Kind
	Mode        string
	Verdict     string
	Fallback    string
}

// alwaysFailClosed are tiers that never fail open, whatever the settings say.
var alwaysFailClosed = map[string]bool{"confidential": true, "crown_jewels": true}
