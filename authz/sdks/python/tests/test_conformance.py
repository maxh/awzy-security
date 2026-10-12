"""The conformance spec every awzy client library runs (ADR 0114): authz/sdks/conformance."""

from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path
from typing import Any

import pytest

from awzy_authz import Allowed, Awzy, Challenged, CheckRequest, CheckResult, Denied, EntityRef, Principal, Resource

from .fake_pdp import FakePdp

SPEC = json.loads((Path(__file__).parents[2] / "conformance" / "scenarios.json").read_text())
PRINCIPAL = Principal(EntityRef(**SPEC["principal"]), EntityRef(**SPEC["on_behalf_of"]))


def fields(r: CheckResult) -> dict[str, Any]:
    """The result's fields under the spec's names."""
    out: dict[str, Any] = {"allowed": r.allowed, "kind": r.kind}
    if isinstance(r, Allowed):
        out.update(mode=r.mode, verdict=r.verdict, fallback=r.fallback, decision_qid=r.decision_qid)
    elif isinstance(r, Denied):
        out.update(decision_qid=r.decision_qid, policies=r.policies)
    elif isinstance(r, Challenged):
        out.update(
            decision_qid=r.decision_qid, challenge_qid=r.challenge_qid, challenges=r.challenges, expires_at=r.expires_at
        )
    return out


@pytest.mark.parametrize("scenario", SPEC["scenarios"], ids=lambda s: s["name"])
def test_conformance(scenario: dict[str, Any]) -> None:
    options = scenario.get("options", {})
    pdp = FakePdp()
    try:
        awzy = Awzy(
            pdp.url,
            budget_ms=options.get("budget_ms", 250),
            fail_open=options.get("fail_open", []),
            fail_open_tiers=options.get("fail_open_tiers", []),
        )
        for i, step in enumerate(scenario["steps"]):
            pdp.answers = {k: step[k] for k in ("evaluation", "challenge") if k in step}
            request, resource = step["request"], step["request"]["resource"]
            before = len(pdp.sent)
            result = awzy.check(
                CheckRequest(
                    action=request["action"],
                    resource=Resource(resource["type"], resource["id"], resource.get("tier")),
                    principal=replace(PRINCIPAL, work_item_id=scenario.get("work_item_id")),
                )
            )
            got = fields(result)
            for key, value in step["expect"].items():
                assert got.get(key) == value, f"step {i}: {key}: {got}"
            if "sent" in step:
                assert pdp.sent[before] == step["sent"], f"step {i}: sent"
    finally:
        pdp.close()


def test_there_are_scenarios() -> None:
    assert len(SPEC["scenarios"]) > 10
