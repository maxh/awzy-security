package awzyauthz

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

const longestWaitSecs = 25

// Options configure a Client.
type Options struct {
	// PDPURL is awzy-pdp's base URL, e.g. http://awzy-pdp.awzy-system.svc:8080.
	PDPURL string
	// Tokens validates callers' tokens; needed for checks that pass an Authorization header.
	Tokens *TokenVerifier
	// ServiceToken gives a token for awzy-pdp, read fresh for each call; see ServiceTokenFromDir.
	ServiceToken func() (string, error)
	// Budget is how long one check may take before the outage fallback decides it. Default 250ms.
	Budget time.Duration
	// FailOpen names actions that may fail open while awzy-pdp can't be reached, on the
	// FailOpenTiers the client portal sets; every other fails closed.
	FailOpen      []string
	FailOpenTiers []string
	// CachedAllowTTL is how long an allow can be replayed during an outage. Default 5 minutes.
	CachedAllowTTL time.Duration
	// Activity records allowed calls in awzy-intake, when set.
	Activity *ActivityLog
	// OnDecision gets each decision, for the app's logs and traces.
	OnDecision func(DecisionNote)
	HTTP       *http.Client
}

// Client is an enforcement point in the app (ADR 0045): it validates the caller, asks awzy-pdp
// within a budget, applies the mode, surfaces challenges, and falls back as awzy-gateway does.
type Client struct {
	opts          Options
	failOpen      map[string]bool
	failOpenTiers map[string]bool

	mu       sync.Mutex
	lastMode string
	allows   map[string]time.Time
}

// ServiceTokenFromDir reads <dir>/awzy-pdp, the service token Kubernetes mounts for the PDP. An
// empty dir means AWZY_SERVICE_TOKEN_DIR, else /var/run/secrets/awzy/tokens.
func ServiceTokenFromDir(dir string) func() (string, error) {
	if dir == "" {
		dir = os.Getenv("AWZY_SERVICE_TOKEN_DIR")
	}
	if dir == "" {
		dir = "/var/run/secrets/awzy/tokens"
	}
	path := filepath.Join(dir, "awzy-pdp")
	return func() (string, error) {
		b, err := os.ReadFile(path)
		return strings.TrimSpace(string(b)), err
	}
}

func New(opts Options) *Client {
	if opts.Budget == 0 {
		opts.Budget = 250 * time.Millisecond
	}
	if opts.CachedAllowTTL == 0 {
		opts.CachedAllowTTL = 5 * time.Minute
	}
	if opts.HTTP == nil {
		opts.HTTP = &http.Client{}
	}
	opts.PDPURL = strings.TrimRight(opts.PDPURL, "/")
	c := &Client{opts: opts, failOpen: map[string]bool{}, failOpenTiers: map[string]bool{}, allows: map[string]time.Time{}}
	for _, a := range opts.FailOpen {
		c.failOpen[a] = true
	}
	for _, t := range opts.FailOpenTiers {
		c.failOpenTiers[t] = true
	}
	return c
}

// CheckRequest is one call to check: the caller's token or an already validated principal, and
// what it does.
type CheckRequest struct {
	Action   string
	Resource Resource
	// Authorization is the caller's Authorization header (Bearer …), validated first.
	Authorization string
	// Principal, instead of Authorization: one the app validated with this library's verifier.
	Principal *Principal
	Context   map[string]any
	// Entities of types the app owns, in Cedar's entity shape. awzy's own come from awzy.
	Entities []any
}

// Check checks one call. Its result says what to do; it never returns an error for awzy's answer.
func (c *Client) Check(ctx context.Context, req CheckRequest) Result {
	ctx, cancel := context.WithTimeout(ctx, c.opts.Budget)
	defer cancel()
	return c.check(ctx, req)
}

// CheckMany checks several calls at once, each its own decision, within one shared budget.
func (c *Client) CheckMany(ctx context.Context, reqs []CheckRequest) []Result {
	ctx, cancel := context.WithTimeout(ctx, c.opts.Budget)
	defer cancel()
	out := make([]Result, len(reqs))
	var wg sync.WaitGroup
	for i, r := range reqs {
		wg.Add(1)
		go func() {
			defer wg.Done()
			out[i] = c.check(ctx, r)
		}()
	}
	wg.Wait()
	return out
}

// Blocked is returned by Run when a call isn't allowed; Result says why and what to do.
type Blocked struct{ Result Result }

func (b *Blocked) Error() string {
	switch b.Result.Kind {
	case KindDenied:
		return fmt.Sprintf("awzy denied this (decision %s)", b.Result.DecisionQID)
	case KindChallenged:
		return fmt.Sprintf("awzy requires %s (challenge %s)", strings.Join(b.Result.Challenges, " and "), b.Result.ChallengeQID)
	default:
		return "awzy couldn't allow this: " + b.Result.Reason
	}
}

// Run runs call if awzy allows it, and records it as activity with its outcome. It returns a
// *Blocked otherwise, without running call.
func Run[T any](ctx context.Context, c *Client, req CheckRequest, call func() (T, error)) (T, error) {
	var zero T
	p, err := c.principal(req)
	if err != nil {
		return zero, &Blocked{Result{Kind: KindUnauthenticated, Reason: err.Error()}}
	}
	req.Principal = &p
	result := c.Check(ctx, req)
	if !result.Allowed {
		return zero, &Blocked{result}
	}
	value, err := call()
	if c.opts.Activity != nil {
		outcome := "success"
		if err != nil {
			outcome = "failure"
		}
		attrs := map[string]any{}
		if result.DecisionQID != "" {
			attrs["decision_qid"] = result.DecisionQID
		}
		c.opts.Activity.Record(Activity{
			Principal: p.Principal, OnBehalfOf: p.OnBehalfOf, Action: req.Action,
			Resource: &EntityRef{req.Resource.Type, req.Resource.ID}, Outcome: outcome, Attributes: attrs,
		})
	}
	return value, err
}

func (c *Client) principal(req CheckRequest) (Principal, error) {
	if req.Principal != nil {
		return *req.Principal, nil
	}
	if c.opts.Tokens == nil {
		return Principal{}, fmt.Errorf("%w: no token verifier is configured", ErrToken)
	}
	return c.opts.Tokens.Principal(req.Authorization)
}

// post sends body as JSON. The returned object is nil when the PDP answered something other than
// a success with a JSON object.
func (c *Client) post(ctx context.Context, path string, body any) (int, map[string]any, error) {
	payload, err := json.Marshal(body)
	if err != nil {
		return 0, nil, err
	}
	r, err := http.NewRequestWithContext(ctx, http.MethodPost, c.opts.PDPURL+path, bytes.NewReader(payload))
	if err != nil {
		return 0, nil, err
	}
	r.Header.Set("content-type", "application/json")
	if c.opts.ServiceToken != nil {
		token, err := c.opts.ServiceToken()
		if err != nil {
			return 0, nil, err
		}
		if token != "" {
			r.Header.Set("authorization", "Bearer "+token)
		}
	}
	res, err := c.opts.HTTP.Do(r)
	if err != nil {
		return 0, nil, err
	}
	defer res.Body.Close()
	raw, err := io.ReadAll(res.Body)
	if err != nil {
		return res.StatusCode, nil, err
	}
	if res.StatusCode < 200 || res.StatusCode > 299 {
		return res.StatusCode, nil, nil
	}
	var d map[string]any
	if json.Unmarshal(raw, &d) != nil {
		return res.StatusCode, nil, nil
	}
	return res.StatusCode, d, nil
}

func text(d map[string]any, key string) string {
	s, _ := d[key].(string)
	return s
}

func strs(v any) []string {
	items, _ := v.([]any)
	out := []string{}
	for _, i := range items {
		if s, ok := i.(string); ok {
			out = append(out, s)
		}
	}
	return out
}

var challengeKinds = map[string]bool{"justification": true, "verification": true, "approval": true, "biometric": true}

func kinds(v any) []string {
	out := []string{}
	for _, k := range strs(v) {
		if challengeKinds[k] {
			out = append(out, k)
		}
	}
	return out
}

func (c *Client) note(req CheckRequest, p Principal, r Result) Result {
	if c.opts.OnDecision != nil {
		c.opts.OnDecision(DecisionNote{
			DecisionQID: r.DecisionQID, Action: req.Action, Resource: EntityRef{req.Resource.Type, req.Resource.ID},
			Principal: p.Principal, OnBehalfOf: p.OnBehalfOf, Result: r.Kind, Mode: r.Mode, Verdict: r.Verdict,
			Fallback: r.Fallback,
		})
	}
	return r
}

func (c *Client) check(ctx context.Context, req CheckRequest) Result {
	p, err := c.principal(req)
	if err != nil {
		return Result{Kind: KindUnauthenticated, Reason: err.Error()}
	}
	resource := EntityRef{req.Resource.Type, req.Resource.ID}
	keyJSON, _ := json.Marshal([]any{p.Principal, p.OnBehalfOf, p.WorkItemID, req.Action, resource})
	key := string(keyJSON)
	reqContext := req.Context
	if reqContext == nil {
		reqContext = map[string]any{}
	}
	entities := req.Entities
	if entities == nil {
		entities = []any{}
	}
	body := map[string]any{
		"principal": p.Principal, "action": req.Action, "resource": resource, "context": reqContext, "entities": entities,
	}
	if p.OnBehalfOf != nil {
		body["on_behalf_of"] = p.OnBehalfOf
	}
	if p.HumanPresent != nil {
		body["human_present"] = *p.HumanPresent
	}
	if p.WorkItemID != "" {
		body["work_item_id"] = p.WorkItemID
	}

	status, d, err := c.post(ctx, "/v1/access/evaluation", body)
	if err != nil {
		reason := "awzy-pdp can't be reached: " + err.Error()
		if errors.Is(ctx.Err(), context.DeadlineExceeded) {
			reason = "awzy-pdp didn't answer within the budget"
		}
		return c.note(req, p, c.fallback(req, key, reason))
	}
	if status == 400 || status == 401 || status == 403 {
		return c.note(req, p, Result{Kind: KindUnavailable, Reason: fmt.Sprintf("awzy-pdp refused the request (%d)", status)})
	}
	if d == nil {
		return c.note(req, p, c.fallback(req, key, fmt.Sprintf("awzy-pdp answered %d", status)))
	}

	// A verdict it can't read is a deny, and a mode it can't read is enforce: both fail safe.
	verdict := text(d, "verdict")
	if verdict != "allow" && verdict != "challenge" {
		verdict = "deny"
	}
	mode := "enforce"
	if text(d, "mode") == "monitor" {
		mode = "monitor"
	}
	c.mu.Lock()
	c.lastMode = mode
	c.mu.Unlock()
	decisionQID := text(d, "qid")
	if mode == "monitor" || verdict == "allow" {
		if verdict == "allow" {
			c.mu.Lock()
			c.allows[key] = time.Now()
			c.mu.Unlock()
		}
		return c.note(req, p, Result{Allowed: true, Kind: KindAllowed, Mode: mode, Verdict: verdict, DecisionQID: decisionQID})
	}
	if verdict == "deny" {
		return c.note(req, p, Result{Kind: KindDenied, DecisionQID: decisionQID, Policies: strs(d["policies"]), Mode: mode, Verdict: verdict})
	}
	challengeQID := text(d, "challenge_qid")
	if challengeQID == "" {
		return c.note(req, p, Result{Kind: KindUnavailable, Reason: "no challenge could be opened; try again shortly", DecisionQID: decisionQID})
	}
	r := Result{Kind: KindChallenged, DecisionQID: decisionQID, ChallengeQID: challengeQID, Challenges: kinds(d["challenges"]), Mode: mode, Verdict: verdict}
	// How long the person has, if awzy says so within what's left of the budget.
	if looked, err := c.lookup(ctx, p, challengeQID, 0); err == nil && looked.Outcome == "pending" {
		r.ExpiresAt = looked.ExpiresAt
	}
	r.wait = func(ctx context.Context) (ChallengeAnswer, error) {
		for {
			wait := longestWaitSecs
			if deadline, ok := ctx.Deadline(); ok {
				wait = min(wait, max(0, int(time.Until(deadline).Seconds())))
			}
			call, cancel := context.WithTimeout(ctx, time.Duration(wait+10)*time.Second)
			answer, err := c.lookup(call, p, challengeQID, wait)
			cancel()
			if err != nil || answer.Outcome != "pending" || ctx.Err() != nil {
				return answer, err
			}
		}
	}
	return c.note(req, p, r)
}

// fallback: the gateway's outage rules, with the app's fail-open actions.
func (c *Client) fallback(req CheckRequest, key, reason string) Result {
	c.mu.Lock()
	lastMode := c.lastMode
	allowedAt, seen := c.allows[key]
	c.mu.Unlock()
	if lastMode == "monitor" {
		return Result{Allowed: true, Kind: KindAllowed, Mode: "monitor", Verdict: "allow", Fallback: "monitor"}
	}
	tier := req.Resource.Tier
	if c.failOpen[req.Action] && tier != "" && !alwaysFailClosed[tier] && c.failOpenTiers[tier] && seen &&
		time.Since(allowedAt) <= c.opts.CachedAllowTTL {
		return Result{Allowed: true, Kind: KindAllowed, Mode: "enforce", Verdict: "allow", Fallback: "cached_allow"}
	}
	return Result{Kind: KindUnavailable, Reason: reason}
}

func (c *Client) lookup(ctx context.Context, p Principal, challengeQID string, waitSecs int) (ChallengeAnswer, error) {
	body := map[string]any{"challenge_qid": challengeQID, "principal": p.Principal, "wait_secs": waitSecs}
	if p.OnBehalfOf != nil {
		body["on_behalf_of"] = p.OnBehalfOf
	}
	status, d, err := c.post(ctx, "/v1/access/challenge", body)
	if err != nil {
		return ChallengeAnswer{}, err
	}
	if d == nil {
		return ChallengeAnswer{}, fmt.Errorf("awzy-pdp answered %d about challenge %s", status, challengeQID)
	}
	switch text(d, "outcome") {
	case "passed":
		return ChallengeAnswer{Outcome: "passed", GrantExpiresAt: text(d, "grant_expires_at")}, nil
	case "rejected":
		return ChallengeAnswer{Outcome: "rejected", ExpiresAt: text(d, "expires_at")}, nil
	case "expired":
		return ChallengeAnswer{Outcome: "expired"}, nil
	default:
		return ChallengeAnswer{Outcome: "pending", ExpiresAt: text(d, "expires_at"), Pending: kinds(d["pending"])}, nil
	}
}
