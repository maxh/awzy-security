package awzyauthz

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"strings"
	"sync"
	"time"
)

const (
	activityQueue = 10_000
	activityBatch = 500
	activityEvery = time.Second
)

// Activity is one thing a principal did, in awzy-intake's app event schema. No argument values.
type Activity struct {
	At         time.Time
	Principal  EntityRef
	OnBehalfOf *EntityRef
	Action     string
	Resource   *EntityRef
	Outcome    string
	// Access says how the resource was touched, and Count how many rows: "listed" for a plan's
	// results.
	Access     string
	Count      *int
	Attributes map[string]any
}

// ActivityLog records what principals do through the app in awzy-intake (ADR 0084), so
// awzy-reasoner scores an agent's calls like any other activity. Best effort: queued, sent a
// batch a second, and dropped when the queue is full or awzy-intake can't be reached.
type ActivityLog struct {
	endpoint, token, system string
	http                    *http.Client
	mu                      sync.Mutex
	queue                   []Activity
	started                 bool
}

func NewActivityLog(url, token, system string) *ActivityLog {
	return &ActivityLog{endpoint: strings.TrimRight(url, "/") + "/v1/events", token: token, system: system, http: &http.Client{}}
}

// Record queues a to be sent. It never waits.
func (l *ActivityLog) Record(a Activity) {
	l.mu.Lock()
	defer l.mu.Unlock()
	if len(l.queue) >= activityQueue {
		return
	}
	if a.At.IsZero() {
		a.At = time.Now()
	}
	l.queue = append(l.queue, a)
	if !l.started {
		l.started = true
		go func() {
			for range time.Tick(activityEvery) {
				l.Flush(context.Background())
			}
		}()
	}
}

// Flush sends what's queued now. Call it before a program exits.
func (l *ActivityLog) Flush(ctx context.Context) {
	for {
		l.mu.Lock()
		n := min(len(l.queue), activityBatch)
		batch := append([]Activity(nil), l.queue[:n]...)
		l.queue = l.queue[n:]
		l.mu.Unlock()
		if len(batch) == 0 {
			return
		}
		events := make([]map[string]any, 0, len(batch))
		for _, a := range batch {
			events = append(events, l.event(a))
		}
		payload, err := json.Marshal(map[string]any{"events": events})
		if err != nil {
			continue
		}
		call, cancel := context.WithTimeout(ctx, 10*time.Second)
		req, err := http.NewRequestWithContext(call, http.MethodPost, l.endpoint, bytes.NewReader(payload))
		if err == nil {
			req.Header.Set("content-type", "application/json")
			req.Header.Set("authorization", "Bearer "+l.token)
			// Best effort: a batch awzy-intake doesn't take is dropped.
			if res, err := l.http.Do(req); err == nil {
				res.Body.Close()
			}
		}
		cancel()
	}
}

func (l *ActivityLog) event(a Activity) map[string]any {
	attrs := a.Attributes
	if attrs == nil {
		attrs = map[string]any{}
	}
	e := map[string]any{
		"at": a.At.UTC().Format("2006-01-02T15:04:05.000Z"), "system": l.system,
		"principal": a.Principal, "action": a.Action, "attributes": attrs,
	}
	if a.OnBehalfOf != nil {
		e["on_behalf_of"] = a.OnBehalfOf
	}
	if a.Resource != nil {
		e["resource"] = a.Resource
	}
	if a.Outcome != "" {
		e["outcome"] = a.Outcome
	}
	if a.Access != "" {
		e["access"] = a.Access
	}
	if a.Count != nil {
		e["count"] = *a.Count
	}
	return e
}
