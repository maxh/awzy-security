"""The design partner's path through this library, against a running install (tools/demo partner):
an app-minted agent's token validated, a check in monitor, a query plan turned into SQL, and the
plan's results sent to awzy-intake. Inputs are AWZY_TEST_* variables; it exits non-zero on the
first thing that isn't as it should be."""

from __future__ import annotations

import os
import sys

from awzy_authz import (
    ActivityLog,
    Allowed,
    Awzy,
    CheckRequest,
    Plan,
    Resource,
    SqlMapping,
    TokenVerifier,
    TrustedApp,
    service_token_from_dir,
    to_postgres,
)


def env(k: str) -> str:
    v = os.environ.get(f"AWZY_TEST_{k}")
    if not v:
        raise SystemExit(f"AWZY_TEST_{k} is not set")
    return v


def ok(what: str) -> None:
    print(f"ok   python: {what}", flush=True)


def fail(what: str) -> None:
    print(f"FAIL python: {what}", flush=True)
    sys.exit(1)


# awzy's Resource as an app that syncs its labels might store it.
RESOURCES = SqlMapping(
    id="documents.id",
    columns={
        "sensitivity": "sensitivity",
        "unclassified": "unclassified",
        "owner": "owner_id",
        "shared_humans": "shared_human_ids",
        "shared_groups": "shared_group_ids",
        "writer_humans": "writer_human_ids",
        "writer_groups": "writer_group_ids",
        "shared_with_everyone": "shared_with_everyone",
    },
    ancestors=("document_ancestors", "document_id", "ancestor_id"),
)


def main() -> None:
    ca = env("CA_FILE")
    tokens = TokenVerifier(
        env("IDP_ISSUER"),
        "awzy",
        env("IDP_JWKS_URL"),
        trusted_apps=[TrustedApp(env("APP_NAME"), env("APP_ISSUER"), env("APP_JWKS_URL"), env("APP_AUDIENCE"))],
        ca_file=ca,
    )
    activity = ActivityLog(env("INTAKE_URL"), env("INTAKE_TOKEN"), env("SYSTEM"))
    awzy = Awzy(
        env("PDP_URL"),
        tokens=tokens,
        service_token=service_token_from_dir(env("SERVICE_TOKEN_DIR")),
        budget_ms=2000,
        activity=activity,
    )
    authorization = f"Bearer {env('AGENT_TOKEN')}"

    # 1. The app-minted agent's token: the agent, acting for its person.
    p = tokens.principal(authorization)
    if p.principal.id != env("AGENT") or p.on_behalf_of is None or p.on_behalf_of.id != env("HUMAN"):
        fail(f"token named {p}")
    ok(f"the app's token is {p.principal.id} acting for {p.on_behalf_of.id if p.on_behalf_of else '?'}")

    # 2. A check, which the install's monitor mode allows whatever the verdict.
    result = awzy.check(CheckRequest("read", Resource("Resource", "demo-check"), authorization=authorization))
    if not isinstance(result, Allowed) or result.mode != "monitor" or not result.decision_qid:
        fail(f"check: {result}")
    else:
        ok(f"a check is decided ({result.verdict}, {result.mode}, {result.decision_qid})")

    # 3. A plan for what the agent may read, its filter mapped to SQL.
    plan = awzy.plan("read", "Resource", authorization=authorization)
    if not isinstance(plan, Plan) or not plan.qid:
        fail(f"plan: {plan}")
        return
    sql = to_postgres(plan.filter, RESOURCES)
    ok(f"a plan ({plan.outcome}) maps to WHERE {sql.where[:80]}{'…' if len(sql.where) > 80 else ''}")

    # 4. Its results sent to awzy-intake as a listed access linked to the plan. Sending is best effort,
    # so this can't see that they arrived; tools/demo checks ClickHouse for them.
    awzy.report_plan(plan, "read", "Resource", 3, authorization=authorization)
    activity.flush()
    ok(f"the plan's results are sent to awzy-intake ({plan.qid})")


if __name__ == "__main__":
    main()
