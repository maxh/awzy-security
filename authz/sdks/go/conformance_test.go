package awzyauthz

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"reflect"
	"sync"
	"testing"
	"time"
)

// The conformance spec every awzy client library runs (ADR 0114): authz/sdks/conformance.

type spec struct {
	Principal  EntityRef  `json:"principal"`
	OnBehalfOf EntityRef  `json:"on_behalf_of"`
	Scenarios  []scenario `json:"scenarios"`
}

type scenario struct {
	Name       string `json:"name"`
	WorkItemID string `json:"work_item_id"`
	Options    struct {
		BudgetMS      int      `json:"budget_ms"`
		FailOpen      []string `json:"fail_open"`
		FailOpenTiers []string `json:"fail_open_tiers"`
	} `json:"options"`
	Steps []struct {
		Evaluation json.RawMessage `json:"evaluation"`
		Challenge  json.RawMessage `json:"challenge"`
		Request    struct {
			Action   string `json:"action"`
			Resource struct {
				Type string `json:"type"`
				ID   string `json:"id"`
				Tier string `json:"tier"`
			} `json:"resource"`
		} `json:"request"`
		Sent   map[string]any `json:"sent"`
		Expect map[string]any `json:"expect"`
	} `json:"steps"`
}

// fakePDP answers as the current step says: a response, a dropped connection (down), or nothing
// for longer than any budget (hang).
type fakePDP struct {
	mu      sync.Mutex
	answers map[string]json.RawMessage
	sent    []map[string]any
}

func (f *fakePDP) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	name := map[string]string{"/v1/access/evaluation": "evaluation", "/v1/access/challenge": "challenge"}[r.URL.Path]
	var body map[string]any
	_ = json.NewDecoder(r.Body).Decode(&body)
	f.mu.Lock()
	if name == "evaluation" {
		f.sent = append(f.sent, body)
	}
	answer := f.answers[name]
	f.mu.Unlock()
	var word string
	if len(answer) == 0 || json.Unmarshal(answer, &word) == nil {
		if word == "hang" {
			time.Sleep(2 * time.Second)
		}
		// down, hang, or nothing set: drop the connection without answering.
		if hj, ok := w.(http.Hijacker); ok {
			if conn, _, err := hj.Hijack(); err == nil {
				conn.Close()
			}
		}
		return
	}
	var resp struct {
		Status int             `json:"status"`
		Body   json.RawMessage `json:"body"`
	}
	_ = json.Unmarshal(answer, &resp)
	if resp.Status == 0 {
		resp.Status = 200
	}
	w.Header().Set("content-type", "application/json")
	w.WriteHeader(resp.Status)
	_, _ = w.Write(resp.Body)
}

// fields is the result's fields under the spec's names.
func fields(r Result) map[string]any {
	out := map[string]any{"allowed": r.Allowed, "kind": string(r.Kind)}
	set := func(k string, v any) {
		switch x := v.(type) {
		case string:
			if x != "" {
				out[k] = x
			}
		case []string:
			items := make([]any, len(x))
			for i, s := range x {
				items[i] = s
			}
			out[k] = items
		}
	}
	switch r.Kind {
	case KindAllowed:
		set("mode", r.Mode)
		set("verdict", r.Verdict)
		set("fallback", r.Fallback)
		set("decision_qid", r.DecisionQID)
	case KindDenied:
		set("decision_qid", r.DecisionQID)
		set("policies", r.Policies)
	case KindChallenged:
		set("decision_qid", r.DecisionQID)
		set("challenge_qid", r.ChallengeQID)
		set("challenges", r.Challenges)
		set("expires_at", r.ExpiresAt)
	}
	return out
}

func TestConformance(t *testing.T) {
	raw, err := os.ReadFile("../conformance/scenarios.json")
	if err != nil {
		t.Fatal(err)
	}
	var s spec
	if err := json.Unmarshal(raw, &s); err != nil {
		t.Fatal(err)
	}
	if len(s.Scenarios) < 10 {
		t.Fatalf("only %d scenarios", len(s.Scenarios))
	}
	principal := Principal{Principal: s.Principal, OnBehalfOf: &s.OnBehalfOf}
	for _, sc := range s.Scenarios {
		t.Run(sc.Name, func(t *testing.T) {
			principal := principal
			principal.WorkItemID = sc.WorkItemID
			pdp := &fakePDP{}
			server := httptest.NewServer(pdp)
			defer server.Close()
			budget := 250 * time.Millisecond
			if sc.Options.BudgetMS > 0 {
				budget = time.Duration(sc.Options.BudgetMS) * time.Millisecond
			}
			c := New(Options{PDPURL: server.URL, Budget: budget, FailOpen: sc.Options.FailOpen, FailOpenTiers: sc.Options.FailOpenTiers})
			for i, step := range sc.Steps {
				pdp.mu.Lock()
				pdp.answers = map[string]json.RawMessage{"evaluation": step.Evaluation, "challenge": step.Challenge}
				before := len(pdp.sent)
				pdp.mu.Unlock()
				r := c.Check(context.Background(), CheckRequest{
					Action:    step.Request.Action,
					Resource:  Resource{step.Request.Resource.Type, step.Request.Resource.ID, step.Request.Resource.Tier},
					Principal: &principal,
				})
				got := fields(r)
				for key, want := range step.Expect {
					if !reflect.DeepEqual(got[key], want) {
						t.Errorf("step %d: %s: got %#v, want %#v (%#v)", i, key, got[key], want, r)
					}
				}
				if step.Sent != nil {
					pdp.mu.Lock()
					sent := pdp.sent[before]
					pdp.mu.Unlock()
					if !reflect.DeepEqual(sent, step.Sent) {
						t.Errorf("step %d: sent %#v, want %#v", i, sent, step.Sent)
					}
				}
			}
		})
	}
}
