package awzyauthz

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"regexp"
	"strings"
)

// Filter is a node of a query plan's filter tree (ADR 0113), as awzy sends it. Op says which
// fields are set: and/or (Args), not (Arg), compare (Attr, Cmp, Value), has (Attr), contains
// (Attr, Value), contains_any/contains_all (Attr, Values), like (Attr, Pattern), in (Ancestor),
// is (ID), unknown (Name), unmappable (Cedar). A row missing an attribute never matches.
type Filter struct {
	Op       string     `json:"op"`
	Args     []Filter   `json:"args,omitempty"`
	Arg      *Filter    `json:"arg,omitempty"`
	Attr     []string   `json:"attr,omitempty"`
	Cmp      string     `json:"cmp,omitempty"`
	Value    any        `json:"value,omitempty"`
	Values   []any      `json:"values,omitempty"`
	Pattern  string     `json:"pattern,omitempty"`
	Ancestor *EntityRef `json:"ancestor,omitempty"`
	ID       *EntityRef `json:"id,omitempty"`
	Name     string     `json:"name,omitempty"`
	Cedar    string     `json:"cedar,omitempty"`
}

var (
	filterTrue  = Filter{Op: "true"}
	filterFalse = Filter{Op: "false"}
	cmpSQL      = map[string]string{"eq": "=", "ne": "<>", "lt": "<", "le": "<=", "gt": ">", "ge": ">="}
)

// ParseFilter reads a filter from awzy's JSON. Anything it doesn't recognize, as from a newer
// awzy, becomes unmappable, so the rows are checked one by one rather than shown.
func ParseFilter(v any) Filter {
	raw, _ := json.Marshal(v)
	unmappable := Filter{Op: "unmappable", Cedar: string(raw)}
	var f Filter
	if json.Unmarshal(raw, &f) != nil {
		return unmappable
	}
	path := len(f.Attr) > 0
	obj, _ := v.(map[string]any)
	switch f.Op {
	case "true", "false":
		return Filter{Op: f.Op}
	case "and", "or":
		items, _ := obj["args"].([]any)
		args := make([]Filter, 0, len(items))
		for _, a := range items {
			args = append(args, ParseFilter(a))
		}
		return Filter{Op: f.Op, Args: args}
	case "not":
		return Filter{Op: "not", Arg: ptr(ParseFilter(obj["arg"]))}
	case "compare":
		if _, ok := cmpSQL[f.Cmp]; ok && path && f.Value != nil {
			return f
		}
	case "has":
		if path {
			return f
		}
	case "contains":
		if path && f.Value != nil {
			return f
		}
	case "contains_any", "contains_all":
		if path {
			return f
		}
	case "like":
		if path {
			return f
		}
	case "in":
		if f.Ancestor != nil {
			return f
		}
	case "is":
		if f.ID != nil {
			return f
		}
	case "unknown":
		if f.Name != "" {
			return f
		}
	}
	return unmappable
}

func ptr[T any](v T) *T { return &v }

// NeedsRowChecks says whether some row has to be checked one by one.
func (f Filter) NeedsRowChecks() bool {
	switch f.Op {
	case "unknown", "unmappable":
		return true
	case "and", "or":
		for _, a := range f.Args {
			if a.NeedsRowChecks() {
				return true
			}
		}
	case "not":
		return f.Arg != nil && f.Arg.NeedsRowChecks()
	}
	return false
}

// Plan is awzy's answer to a list or search: what to show, as a filter, and what it rests on.
type Plan struct {
	QID     string
	Outcome string
	Filter  Filter
	// Apply is the filter to use now, with the mode applied: every row in monitor, no row while
	// a challenge is pending, otherwise the plan's own.
	Apply               Filter
	Mode                string
	Challenges          []string
	ChallengeCouldWiden bool
	NeedsRowChecks      bool
	Attributes          []string
	MaxAgeSecs          int
	Pinned              bool
}

// PlanRequest asks which records of ResourceType the caller may act on.
type PlanRequest struct {
	Action        string
	ResourceType  string
	Authorization string
	Principal     *Principal
	Context       map[string]any
	Entities      []any
	// PlanQID is the first page's plan, to page through with the same filter.
	PlanQID string
}

// ErrUnavailable is returned by Plan when awzy can't answer: show nothing, or check rows one by
// one.
var ErrUnavailable = errors.New("awzy is unavailable")

// Plan asks which records the caller may act on, as a filter for the app's own query (ADR 0113).
func (c *Client) Plan(ctx context.Context, req PlanRequest) (Plan, error) {
	p, err := c.principal(CheckRequest{Authorization: req.Authorization, Principal: req.Principal})
	if err != nil {
		return Plan{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, c.opts.Budget)
	defer cancel()
	body := map[string]any{"principal": p.Principal, "action": req.Action, "resource_type": req.ResourceType,
		"context": orEmpty(req.Context), "entities": orNone(req.Entities)}
	if p.OnBehalfOf != nil {
		body["on_behalf_of"] = p.OnBehalfOf
	}
	if p.HumanPresent != nil {
		body["human_present"] = *p.HumanPresent
	}
	if req.PlanQID != "" {
		body["plan_qid"] = req.PlanQID
	}
	status, d, err := c.post(ctx, "/v1/access/plan", body)
	if err != nil {
		return Plan{}, fmt.Errorf("%w: %v", ErrUnavailable, err)
	}
	if d == nil {
		return Plan{}, fmt.Errorf("%w: awzy-pdp answered %d", ErrUnavailable, status)
	}
	// An outcome it can't read shows nothing, as a verdict it can't read denies.
	outcome := text(d, "outcome")
	switch outcome {
	case "always_allowed", "always_denied", "challenge", "conditional":
	default:
		outcome = "always_denied"
	}
	mode := "enforce"
	if text(d, "mode") == "monitor" {
		mode = "monitor"
	}
	c.mu.Lock()
	c.lastMode = mode
	c.mu.Unlock()
	f := ParseFilter(d["filter"])
	apply := filterFalse
	switch {
	case mode == "monitor", outcome == "always_allowed":
		apply = filterTrue
	case outcome == "conditional":
		apply = f
	}
	maxAge, _ := d["max_age_secs"].(float64)
	widen, _ := d["challenge_could_widen"].(bool)
	pinned, _ := d["pinned"].(bool)
	return Plan{QID: text(d, "qid"), Outcome: outcome, Filter: f, Apply: apply, Mode: mode, Challenges: strs(d["challenges"]),
		ChallengeCouldWiden: widen, NeedsRowChecks: f.NeedsRowChecks(), Attributes: strs(d["attributes"]),
		MaxAgeSecs: int(maxAge), Pinned: pinned}, nil
}

// ReportPlan records what a plan's query returned in awzy-intake, as a listed access of count
// rows linked to the plan, so a bulk read through a plan still moves risk. Call it for every page.
func (c *Client) ReportPlan(req PlanRequest, plan Plan, count int) error {
	p, err := c.principal(CheckRequest{Authorization: req.Authorization, Principal: req.Principal})
	if err != nil {
		return err
	}
	if c.opts.Activity != nil {
		c.opts.Activity.Record(Activity{Principal: p.Principal, OnBehalfOf: p.OnBehalfOf, Action: req.Action,
			Resource: &EntityRef{req.ResourceType, ""}, Outcome: "success", Access: "listed", Count: &count,
			Attributes: map[string]any{"plan_qid": plan.QID}})
	}
	return nil
}

func orEmpty(m map[string]any) map[string]any {
	if m == nil {
		return map[string]any{}
	}
	return m
}

func orNone(s []any) []any {
	if s == nil {
		return []any{}
	}
	return s
}

// SQLMapping says how a type's attributes sit in a Postgres table. ID is the row's ID column,
// qualified with its table (customers.id) so the ancestor join's subquery can't mistake it.
// Columns maps each attribute path, joined with dots, to its column. Ancestors, for in, is a table
// holding each row's ancestors.
type SQLMapping struct {
	ID        string
	Columns   map[string]string
	Ancestors *AncestorTable
}

type AncestorTable struct{ Table, Row, Ancestor string }

// SQL is a WHERE clause with $1, $2, … for Params. NeedsRowChecks says some of the filter couldn't
// be mapped: Where is then a superset, so check each row it returns one by one.
type SQL struct {
	Where          string
	Params         []any
	NeedsRowChecks bool
	Unmapped       []string
}

// LikePattern turns Cedar's like pattern into SQL's: * is %, \* a literal star, and SQL's own
// %, _, and \ escaped.
func LikePattern(cedar string) string {
	var b strings.Builder
	r := []rune(cedar)
	for i := 0; i < len(r); i++ {
		switch c := r[i]; {
		case c == '\\' && i+1 < len(r) && r[i+1] == '*':
			b.WriteRune('*')
			i++
		case c == '*':
			b.WriteRune('%')
		case c == '%' || c == '_' || c == '\\':
			b.WriteRune('\\')
			b.WriteRune(c)
		default:
			b.WriteRune(c)
		}
	}
	return b.String()
}

var plainName = regexp.MustCompile(`^[a-z_][a-z0-9_]*(\.[a-z_][a-z0-9_]*)?$`)

func quote(name string) string {
	if plainName.MatchString(name) {
		return name
	}
	return `"` + strings.ReplaceAll(name, `"`, `""`) + `"`
}

// ToPostgres turns a plan's filter into a Postgres WHERE clause: the reference mapper. What it
// can't map is replaced by whatever keeps the clause a superset of the rows the filter matches
// (TRUE where it allows rows, FALSE under a not), and reported.
func ToPostgres(f Filter, m SQLMapping, firstParam int) SQL {
	if firstParam == 0 {
		firstParam = 1
	}
	out := SQL{Params: []any{}, Unmapped: []string{}}
	param := func(v any) string {
		out.Params = append(out.Params, v)
		return fmt.Sprintf("$%d", firstParam+len(out.Params)-1)
	}
	value := func(v any) any {
		if e, ok := v.(map[string]any); ok {
			return e["id"]
		}
		return v
	}
	values := func(vs []any) []any {
		o := make([]any, 0, len(vs))
		for _, v := range vs {
			o = append(o, value(v))
		}
		return o
	}
	var walk func(n Filter, positive bool) string
	walk = func(n Filter, positive bool) string {
		cant := func(why string) string {
			out.Unmapped = append(out.Unmapped, why)
			if positive {
				return "TRUE"
			}
			return "FALSE"
		}
		switch n.Op {
		case "true":
			return "TRUE"
		case "false":
			return "FALSE"
		case "and", "or":
			if len(n.Args) == 0 {
				if n.Op == "and" {
					return "TRUE"
				}
				return "FALSE"
			}
			parts := make([]string, 0, len(n.Args))
			for _, a := range n.Args {
				parts = append(parts, walk(a, positive))
			}
			return "(" + strings.Join(parts, " "+strings.ToUpper(n.Op)+" ") + ")"
		case "not":
			if n.Arg == nil {
				return cant("not without an argument")
			}
			return "NOT " + walk(*n.Arg, !positive)
		case "is":
			return quote(m.ID) + " = " + param(n.ID.ID)
		case "in":
			if m.Ancestors == nil {
				return cant("no ancestors table for in")
			}
			p := param(n.Ancestor.ID)
			t, id := quote(m.Ancestors.Table), quote(m.ID)
			return fmt.Sprintf("(%s = %s OR EXISTS (SELECT 1 FROM %s WHERE %s.%s = %s AND %s.%s = %s))",
				id, p, t, t, quote(m.Ancestors.Row), id, t, quote(m.Ancestors.Ancestor), p)
		case "unknown":
			return cant(n.Name + " is worked out by awzy for each row")
		case "unmappable":
			return cant("Cedar the filter can't express: " + n.Cedar)
		}
		column, ok := m.Columns[strings.Join(n.Attr, ".")]
		if !ok {
			return cant("no column for " + strings.Join(n.Attr, "."))
		}
		c := quote(column)
		switch n.Op {
		case "compare":
			return c + " " + cmpSQL[n.Cmp] + " " + param(value(n.Value))
		case "has":
			return c + " IS NOT NULL"
		case "contains":
			return param(value(n.Value)) + " = ANY(" + c + ")"
		case "contains_any":
			return c + " && " + param(values(n.Values))
		case "contains_all":
			return c + " @> " + param(values(n.Values))
		case "like":
			return c + " LIKE " + param(LikePattern(n.Pattern))
		}
		return cant("a filter node it doesn't know: " + n.Op)
	}
	out.Where = walk(f, true)
	out.NeedsRowChecks = len(out.Unmapped) > 0
	return out
}
