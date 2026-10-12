"""An enforcement point in the app (ADR 0045): validates the caller, asks awzy-pdp within a
budget, applies the mode, surfaces challenges, and falls back as awzy-gateway does."""

from __future__ import annotations

import json
import os
import threading
import time
from collections.abc import Callable, Mapping, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from typing import Any, TypeVar

import httpx

from .activity import Activity, ActivityLog
from .plans import FALSE, TRUE, Plan, PlanOutcome, needs_row_checks, parse_filter
from .token import TokenError, TokenVerifier
from .types import (
    ALWAYS_FAIL_CLOSED,
    Allowed,
    ChallengeAnswer,
    Challenged,
    ChallengeKind,
    CheckResult,
    DecisionNote,
    Denied,
    EntityRef,
    Mode,
    Principal,
    Resource,
    Unauthenticated,
    Unavailable,
    Verdict,
)

T = TypeVar("T")

_KINDS: frozenset[str] = frozenset({"justification", "verification", "approval", "biometric"})
_LONGEST_WAIT_SECS = 25


def _kinds(v: Any) -> list[ChallengeKind]:
    return [k for k in v if isinstance(k, str) and k in _KINDS] if isinstance(v, list) else []  # type: ignore[misc]


def _strings(v: Any) -> list[str]:
    return [s for s in v if isinstance(s, str)] if isinstance(v, list) else []


def _text(d: Mapping[str, Any], key: str) -> str | None:
    v = d.get(key)
    return v if isinstance(v, str) else None


def service_token_from_dir(dir: str | None = None) -> Callable[[], str]:
    """Reads ``<dir>/awzy-pdp``, the service token Kubernetes mounts for the PDP."""
    path = os.path.join(dir or os.environ.get("AWZY_SERVICE_TOKEN_DIR", "/var/run/secrets/awzy/tokens"), "awzy-pdp")

    def read() -> str:
        with open(path, encoding="utf-8") as f:
            return f.read().strip()

    return read


class Blocked(Exception):
    """Raised by ``Awzy.run`` when a call isn't allowed; ``result`` says why and what to do."""

    def __init__(self, result: Denied | Challenged | Unavailable | Unauthenticated) -> None:
        if isinstance(result, Denied):
            message = f"awzy denied this (decision {result.decision_qid})"
        elif isinstance(result, Challenged):
            message = f"awzy requires {' and '.join(result.challenges)} (challenge {result.challenge_qid})"
        else:
            message = f"awzy couldn't allow this: {result.reason}"
        super().__init__(message)
        self.result = result


@dataclass(frozen=True)
class CheckRequest:
    """One call to check: the caller's token or an already validated principal, and what it does."""

    action: str
    resource: Resource
    #: The caller's ``Authorization`` header (``Bearer …``), validated before anything is asked.
    authorization: str | None = None
    #: Instead of ``authorization``: a principal the app validated with this library's verifier.
    principal: Principal | None = None
    context: Mapping[str, Any] | None = None
    #: Entities of types the app owns, in Cedar's entity shape. awzy's own come from awzy.
    entities: Sequence[Any] = ()


class Awzy:
    """Checks calls with awzy-pdp.

    ``budget_ms`` is how long one check may take before the outage fallback decides it (default
    250). ``fail_open`` names actions that may fail open while awzy-pdp can't be reached, on the
    ``fail_open_tiers`` the client portal sets; every other fails closed. ``service_token`` gives a
    token for awzy-pdp, read fresh for each call.
    """

    def __init__(
        self,
        pdp_url: str,
        *,
        tokens: TokenVerifier | None = None,
        service_token: Callable[[], str | None] | None = None,
        budget_ms: int = 250,
        fail_open: Sequence[str] = (),
        fail_open_tiers: Sequence[str] = (),
        cached_allow_ttl_s: float = 300.0,
        activity: ActivityLog | None = None,
        on_decision: Callable[[DecisionNote], None] | None = None,
        http: httpx.Client | None = None,
    ) -> None:
        self._url = pdp_url.rstrip("/")
        self._tokens = tokens
        self._service_token = service_token
        self._budget = budget_ms / 1000
        self._fail_open = frozenset(fail_open)
        self._fail_open_tiers = frozenset(fail_open_tiers)
        self._ttl = cached_allow_ttl_s
        self._activity = activity
        self._on_decision = on_decision
        self._http = http or httpx.Client()
        #: The last mode awzy-pdp reported: a monitoring install keeps allowing through an outage.
        self._last_mode: Mode | None = None
        #: When awzy-pdp last allowed each request, for replaying during an outage.
        self._allows: dict[str, float] = {}
        self._lock = threading.Lock()

    # ---- Checks

    def check(self, req: CheckRequest) -> CheckResult:
        """Checks one call. Never raises for awzy's answer: the result says what to do."""
        return self._check(req, time.monotonic() + self._budget)

    def check_many(self, reqs: Sequence[CheckRequest]) -> list[CheckResult]:
        """Checks several calls at once, each its own decision, within one shared budget."""
        deadline = time.monotonic() + self._budget
        with ThreadPoolExecutor(max_workers=max(1, min(len(reqs), 16))) as pool:
            return list(pool.map(lambda r: self._check(r, deadline), reqs))

    def run(self, req: CheckRequest, call: Callable[[], T]) -> T:
        """Runs ``call`` if awzy allows it, and records it as activity with its outcome. Raises
        ``Blocked`` otherwise, without running it."""
        try:
            principal = self._principal(req.principal, req.authorization)
        except TokenError as e:
            raise Blocked(Unauthenticated(str(e))) from e
        result = self.check(
            CheckRequest(req.action, req.resource, principal=principal, context=req.context, entities=req.entities)
        )
        if not isinstance(result, Allowed):
            raise Blocked(result)

        def record(outcome: str) -> None:
            if self._activity:
                self._activity.record(
                    Activity(
                        principal=principal.principal,
                        on_behalf_of=principal.on_behalf_of,
                        action=req.action,
                        resource=EntityRef(req.resource.type, req.resource.id),
                        outcome=outcome,
                        attributes={"decision_qid": result.decision_qid} if result.decision_qid else {},
                    )
                )

        try:
            value = call()
        except BaseException:
            record("failure")
            raise
        record("success")
        return value

    # ---- Plans

    def plan(
        self,
        action: str,
        resource_type: str,
        *,
        authorization: str | None = None,
        principal: Principal | None = None,
        context: Mapping[str, Any] | None = None,
        entities: Sequence[Any] = (),
        plan_qid: str | None = None,
    ) -> Plan | Unavailable | Unauthenticated:
        """Which records of ``resource_type`` the caller may act on, as a filter for the app's own
        query (ADR 0113). When awzy can't answer within the budget the result is ``Unavailable``:
        show nothing, or check rows one by one. Pass the first page's ``qid`` as ``plan_qid`` to
        page with the same filter."""
        try:
            p = self._principal(principal, authorization)
        except TokenError as e:
            return Unauthenticated(str(e))
        body: dict[str, Any] = {
            "principal": p.principal.to_json(),
            "action": action,
            "resource_type": resource_type,
            "context": dict(context or {}),
            "entities": list(entities),
        }
        if p.on_behalf_of:
            body["on_behalf_of"] = p.on_behalf_of.to_json()
        if p.human_present is not None:
            body["human_present"] = p.human_present
        if plan_qid:
            body["plan_qid"] = plan_qid
        try:
            status, d = self._post("/v1/access/plan", body, self._budget)
        except httpx.TimeoutException:
            return Unavailable("awzy-pdp didn't answer within the budget")
        except httpx.HTTPError as e:
            return Unavailable(f"awzy-pdp can't be reached: {e}")
        if d is None:
            return Unavailable(f"awzy-pdp answered {status}")
        wire = _text(d, "outcome")
        # An outcome it can't read shows nothing, as a verdict it can't read denies.
        outcome: PlanOutcome = (
            wire if wire in ("always_allowed", "always_denied", "challenge", "conditional") else "always_denied"  # type: ignore[assignment]
        )
        mode: Mode = "monitor" if _text(d, "mode") == "monitor" else "enforce"
        with self._lock:
            self._last_mode = mode
        f = parse_filter(d.get("filter"))
        enforced = TRUE if outcome == "always_allowed" else f if outcome == "conditional" else FALSE
        max_age = d.get("max_age_secs")
        return Plan(
            qid=_text(d, "qid") or "",
            outcome=outcome,
            filter=f,
            apply=TRUE if mode == "monitor" else enforced,
            mode=mode,
            challenges=_strings(d.get("challenges")),
            challenge_could_widen=d.get("challenge_could_widen") is True,
            needs_row_checks=needs_row_checks(f),
            attributes=_strings(d.get("attributes")),
            max_age_secs=max_age if isinstance(max_age, int) else 0,
            pinned=d.get("pinned") is True,
        )

    def report_plan(
        self,
        plan: Plan,
        action: str,
        resource_type: str,
        count: int,
        *,
        authorization: str | None = None,
        principal: Principal | None = None,
    ) -> None:
        """Records what a plan's query returned in awzy-intake, as a ``listed`` access of ``count``
        rows linked to the plan, so a bulk read through a plan still moves risk. Call it for every
        page shown."""
        p = self._principal(principal, authorization)
        if self._activity:
            self._activity.record(
                Activity(
                    principal=p.principal,
                    on_behalf_of=p.on_behalf_of,
                    action=action,
                    resource=EntityRef(resource_type, ""),
                    outcome="success",
                    access="listed",
                    count=count,
                    attributes={"plan_qid": plan.qid},
                )
            )

    def _principal(self, principal: Principal | None, authorization: str | None) -> Principal:
        if principal:
            return principal
        if not self._tokens:
            raise TokenError("no token verifier is configured")
        return self._tokens.principal(authorization)

    def _post(self, path: str, body: Any, timeout: float) -> tuple[int, dict[str, Any] | None]:
        headers = {"content-type": "application/json"}
        token = self._service_token() if self._service_token else None
        if token:
            headers["authorization"] = f"Bearer {token}"
        res = self._http.post(
            f"{self._url}{path}", content=json.dumps(body), headers=headers, timeout=httpx.Timeout(max(timeout, 0.001))
        )
        if not res.is_success:
            return res.status_code, None
        try:
            data = res.json()
        except ValueError:
            return res.status_code, None
        return res.status_code, data if isinstance(data, dict) else None

    def _note(self, req: CheckRequest, p: Principal, result: CheckResult, **extra: Any) -> CheckResult:
        if self._on_decision:
            self._on_decision(
                DecisionNote(
                    action=req.action,
                    resource=EntityRef(req.resource.type, req.resource.id),
                    principal=p.principal,
                    on_behalf_of=p.on_behalf_of,
                    result=result.kind,
                    **extra,
                )
            )
        return result

    def _check(self, req: CheckRequest, deadline: float) -> CheckResult:
        try:
            p = self._principal(req.principal, req.authorization)
        except TokenError as e:
            return Unauthenticated(str(e))
        resource = {"type": req.resource.type, "id": req.resource.id}
        key = json.dumps(
            [p.principal.to_json(), p.on_behalf_of.to_json() if p.on_behalf_of else None, req.action, resource]
        )
        body: dict[str, Any] = {
            "principal": p.principal.to_json(),
            "action": req.action,
            "resource": resource,
            "context": dict(req.context or {}),
            "entities": list(req.entities),
        }
        if p.on_behalf_of:
            body["on_behalf_of"] = p.on_behalf_of.to_json()
        if p.human_present is not None:
            body["human_present"] = p.human_present

        try:
            status, d = self._post("/v1/access/evaluation", body, deadline - time.monotonic())
        except httpx.TimeoutException:
            fallback = self._fallback(req, key, "awzy-pdp didn't answer within the budget")
            return self._note(req, p, fallback, fallback=getattr(fallback, "fallback", None))
        except httpx.HTTPError as e:
            fallback = self._fallback(req, key, f"awzy-pdp can't be reached: {e}")
            return self._note(req, p, fallback, fallback=getattr(fallback, "fallback", None))
        if status in (400, 401, 403):
            return self._note(req, p, Unavailable(f"awzy-pdp refused the request ({status})"))
        if d is None:
            fallback = self._fallback(req, key, f"awzy-pdp answered {status}")
            return self._note(req, p, fallback, fallback=getattr(fallback, "fallback", None))

        # A verdict it can't read is a deny, and a mode it can't read is enforce: both fail safe.
        wire = _text(d, "verdict")
        verdict: Verdict = wire if wire in ("allow", "challenge") else "deny"  # type: ignore[assignment]
        mode: Mode = "monitor" if _text(d, "mode") == "monitor" else "enforce"
        with self._lock:
            self._last_mode = mode
        decision_qid = _text(d, "qid") or ""
        challenge_qid = _text(d, "challenge_qid")
        noted = {"decision_qid": decision_qid, "mode": mode, "verdict": verdict}
        if mode == "monitor" or verdict == "allow":
            if verdict == "allow":
                with self._lock:
                    self._allows[key] = time.monotonic()
            return self._note(req, p, Allowed(mode=mode, verdict=verdict, decision_qid=decision_qid), **noted)
        if verdict == "deny":
            return self._note(req, p, Denied(decision_qid, _strings(d.get("policies"))), **noted)
        if not challenge_qid:
            return self._note(req, p, Unavailable("no challenge could be opened; try again shortly"), **noted)
        expires_at = None
        # How long the person has, if awzy says so within what's left of the budget.
        try:
            looked = self._lookup(p, challenge_qid, 0, deadline - time.monotonic())
            if looked.outcome == "pending":
                expires_at = looked.expires_at
        except (httpx.HTTPError, RuntimeError):
            pass

        def wait(timeout_s: float | None = None) -> ChallengeAnswer:
            until = None if timeout_s is None else time.monotonic() + timeout_s
            while True:
                left = (
                    _LONGEST_WAIT_SECS
                    if until is None
                    else max(0, min(_LONGEST_WAIT_SECS, int(until - time.monotonic())))
                )
                answer = self._lookup(p, challenge_qid, left, left + 10)
                if answer.outcome != "pending" or (until is not None and time.monotonic() >= until):
                    return answer

        challenged = Challenged(decision_qid, challenge_qid, _kinds(d.get("challenges")), expires_at, wait)
        return self._note(req, p, challenged, **noted)

    def _fallback(self, req: CheckRequest, key: str, reason: str) -> CheckResult:
        """The gateway's outage rules, with the app's fail-open actions."""
        with self._lock:
            last_mode = self._last_mode
            allowed_at = self._allows.get(key)
        if last_mode == "monitor":
            return Allowed(mode="monitor", verdict="allow", fallback="monitor")
        tier = req.resource.tier
        if (
            req.action in self._fail_open
            and tier is not None
            and tier not in ALWAYS_FAIL_CLOSED
            and tier in self._fail_open_tiers
            and allowed_at is not None
            and time.monotonic() - allowed_at <= self._ttl
        ):
            return Allowed(mode="enforce", verdict="allow", fallback="cached_allow")
        return Unavailable(reason)

    def _lookup(self, p: Principal, challenge_qid: str, wait_secs: int, timeout: float) -> ChallengeAnswer:
        body: dict[str, Any] = {
            "challenge_qid": challenge_qid,
            "principal": p.principal.to_json(),
            "wait_secs": wait_secs,
        }
        if p.on_behalf_of:
            body["on_behalf_of"] = p.on_behalf_of.to_json()
        status, d = self._post("/v1/access/challenge", body, timeout)
        if d is None:
            raise RuntimeError(f"awzy-pdp answered {status} about challenge {challenge_qid}")
        outcome = _text(d, "outcome")
        if outcome == "passed":
            return ChallengeAnswer("passed", grant_expires_at=_text(d, "grant_expires_at"))
        if outcome == "rejected":
            return ChallengeAnswer("rejected", expires_at=_text(d, "expires_at"))
        if outcome == "expired":
            return ChallengeAnswer("expired")
        return ChallengeAnswer("pending", expires_at=_text(d, "expires_at"), pending=_kinds(d.get("pending")))
