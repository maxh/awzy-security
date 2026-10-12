package awzyauthz

import (
	"reflect"
	"testing"
)

var customers = SQLMapping{
	ID:        "customers.id",
	Columns:   map[string]string{"tenant": "tenant", "labels": "labels", "owner": "owner_id"},
	Ancestors: &AncestorTable{"customer_regions", "customer_id", "region_id"},
}

func TestFiltersMapToPostgresWithParameters(t *testing.T) {
	f := ParseFilter(map[string]any{"op": "and", "args": []any{
		map[string]any{"op": "compare", "attr": []any{"tenant"}, "cmp": "eq", "value": "acme"},
		map[string]any{"op": "not", "arg": map[string]any{"op": "contains", "attr": []any{"labels"}, "value": "legal-hold"}},
		map[string]any{"op": "in", "ancestor": map[string]any{"type": "Region", "id": "emea"}},
	}})
	sql := ToPostgres(f, customers, 1)
	want := "(tenant = $1 AND NOT $2 = ANY(labels) AND (customers.id = $3 OR EXISTS (SELECT 1 FROM customer_regions " +
		"WHERE customer_regions.customer_id = customers.id AND customer_regions.region_id = $3)))"
	if sql.Where != want || !reflect.DeepEqual(sql.Params, []any{"acme", "legal-hold", "emea"}) || sql.NeedsRowChecks {
		t.Fatalf("%q %v", sql.Where, sql.Params)
	}
}

func TestWhatItCantMapKeepsTheClauseASuperset(t *testing.T) {
	unknown := map[string]any{"op": "unknown", "name": "context.assigned"}
	if w := ToPostgres(ParseFilter(map[string]any{"op": "or", "args": []any{unknown, map[string]any{"op": "has", "attr": []any{"nickname"}}}}), customers, 1).Where; w != "(TRUE OR TRUE)" {
		t.Fatal(w)
	}
	sql := ToPostgres(ParseFilter(map[string]any{"op": "not", "arg": unknown}), customers, 1)
	if sql.Where != "NOT FALSE" || !sql.NeedsRowChecks {
		t.Fatalf("%+v", sql)
	}
	if ParseFilter(map[string]any{"op": "regex"}).Op != "unmappable" || ParseFilter("x").Op != "unmappable" {
		t.Fatal("an unknown node isn't unmappable")
	}
}

func TestLikeMatchesCedarsWildcards(t *testing.T) {
	if LikePattern("*.pdf") != "%.pdf" || LikePattern(`50%_off\*`) != `50\%\_off*` {
		t.Fatal(LikePattern(`50%_off\*`))
	}
}
