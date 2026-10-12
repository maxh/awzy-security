from __future__ import annotations

from awzy_authz import Awzy, EntityRef, Principal, SqlMapping, like_pattern, parse_filter, to_postgres
from awzy_authz.activity import ActivityLog

from .fake_pdp import FakePdp

CUSTOMERS = SqlMapping(
    id="customers.id",
    columns={"tenant": "tenant", "tier": "tier", "labels": "labels", "owner": "owner_id"},
    ancestors=("customer_regions", "customer_id", "region_id"),
)
BOT = Principal(EntityRef("ServiceAccount", "acme-crm.renewals-bot"), EntityRef("Human", "dana"))


def test_filters_map_to_postgres_with_parameters() -> None:
    sql = to_postgres(
        {
            "op": "and",
            "args": [
                {"op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme"},
                {"op": "not", "arg": {"op": "contains", "attr": ["labels"], "value": "legal-hold"}},
                {"op": "in", "ancestor": {"type": "Region", "id": "emea"}},
            ],
        },
        CUSTOMERS,
    )
    assert sql.where == (
        "(tenant = %s AND NOT %s = ANY(labels) AND (customers.id = %s OR EXISTS (SELECT 1 FROM customer_regions "
        "WHERE customer_regions.customer_id = customers.id AND customer_regions.region_id = %s)))"
    )
    assert sql.params == ["acme", "legal-hold", "emea", "emea"]
    assert not sql.needs_row_checks


def test_what_it_cant_map_keeps_the_clause_a_superset() -> None:
    unknown = {"op": "unknown", "name": "context.assigned"}
    assert to_postgres({"op": "or", "args": [unknown, {"op": "has", "attr": ["nickname"]}]}, CUSTOMERS).where == (
        "(TRUE OR TRUE)"
    )
    sql = to_postgres({"op": "not", "arg": unknown}, CUSTOMERS)
    assert (sql.where, sql.needs_row_checks) == ("NOT FALSE", True)


def test_like_and_unknown_nodes() -> None:
    assert like_pattern("*.pdf") == "%.pdf"
    assert like_pattern("50%_off\\*") == "50\\%\\_off*"
    assert parse_filter({"op": "regex"})["op"] == "unmappable"


def test_a_plan_applies_its_mode_and_its_results_are_reported() -> None:
    pdp = FakePdp()
    intake = FakePdp()
    try:
        filt = {"op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme"}
        body = {
            "qid": "plan_0192a000-0000-7000-8000-000000000009",
            "outcome": "conditional",
            "filter": filt,
            "mode": "enforce",
            "challenges": [],
            "attributes": ["tenant"],
            "max_age_secs": 60,
            "pinned": False,
        }
        pdp.answers = {"plan": {"body": body}}
        activity = ActivityLog(intake.url, "t", "acme-crm")
        awzy = Awzy(pdp.url, activity=activity)
        plan = awzy.plan("view_customer", "Customer", principal=BOT)
        assert plan.apply == filt  # type: ignore[union-attr]
        pdp.answers = {"plan": {"body": {**body, "mode": "monitor"}}}
        assert awzy.plan("view_customer", "Customer", principal=BOT).apply == {"op": "true"}  # type: ignore[union-attr]
        pdp.answers = {"plan": {"body": {**body, "outcome": "maybe"}}}
        assert awzy.plan("view_customer", "Customer", principal=BOT).apply == {"op": "false"}  # type: ignore[union-attr]

        intake.answers = {"plan": {"body": {}}}
        awzy.report_plan(plan, "view_customer", "Customer", 40, principal=BOT)  # type: ignore[arg-type]
        activity.flush()
    finally:
        pdp.close()
        intake.close()
