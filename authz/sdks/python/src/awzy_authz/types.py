"""What a check asks and what it answers."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any, Literal

Mode = Literal["monitor", "enforce"]
Verdict = Literal["allow", "deny", "challenge"]
ChallengeKind = Literal["justification", "verification", "approval", "biometric"]

#: Tiers that never fail open, whatever the settings say.
ALWAYS_FAIL_CLOSED = frozenset({"confidential", "crown_jewels"})


@dataclass(frozen=True)
class EntityRef:
    """A typed reference to an entity: ``EntityRef("Human", "alice")``."""

    type: str
    id: str

    def to_json(self) -> dict[str, str]:
        return {"type": self.type, "id": self.id}


@dataclass(frozen=True)
class Principal:
    """Who a check is about, from a validated token: the principal, and the person it acts for."""

    principal: EntityRef
    on_behalf_of: EntityRef | None = None
    #: Whether a person is there to answer a challenge; None leaves it to the PDP.
    human_present: bool | None = None
    #: The work item (a ticket or task, in your own systems' IDs) the principal says it's doing,
    #: from the token's ``awzy_work_item_id``. The PDP then decides assignment by it alone.
    work_item_id: str | None = None


@dataclass(frozen=True)
class Resource:
    """A resource, with its tier when the app knows it: only a known tier can fail open."""

    type: str
    id: str
    tier: str | None = None


@dataclass(frozen=True)
class Allowed:
    """The call may go ahead. In monitor mode, ``verdict`` is what enforcing would have done."""

    mode: Mode
    verdict: Verdict
    decision_qid: str | None = None
    #: Set when the PDP couldn't be asked: why the call is allowed anyway.
    fallback: Literal["monitor", "cached_allow"] | None = None
    allowed: Literal[True] = True
    kind: Literal["allowed"] = "allowed"


@dataclass(frozen=True)
class Denied:
    decision_qid: str
    policies: list[str]
    allowed: Literal[False] = False
    kind: Literal["denied"] = "denied"


@dataclass(frozen=True)
class ChallengeAnswer:
    """What became of a challenge: ``passed`` (retry), ``rejected``, ``expired``, or ``pending``."""

    outcome: Literal["passed", "rejected", "expired", "pending"]
    expires_at: str | None = None
    grant_expires_at: str | None = None
    pending: list[ChallengeKind] = field(default_factory=list)


@dataclass(frozen=True)
class Challenged:
    """The person must answer first. Call ``wait()``, then retry the same call when it's passed."""

    decision_qid: str
    challenge_qid: str
    challenges: list[ChallengeKind]
    expires_at: str | None
    wait: Callable[..., ChallengeAnswer] = field(repr=False, compare=False)
    allowed: Literal[False] = False
    kind: Literal["challenged"] = "challenged"


@dataclass(frozen=True)
class Unavailable:
    """awzy couldn't decide, and nothing allows the call without it. Retry shortly."""

    reason: str
    allowed: Literal[False] = False
    kind: Literal["unavailable"] = "unavailable"


@dataclass(frozen=True)
class Unauthenticated:
    """The caller's token isn't valid: nothing was asked."""

    reason: str
    allowed: Literal[False] = False
    kind: Literal["unauthenticated"] = "unauthenticated"


CheckResult = Allowed | Denied | Challenged | Unavailable | Unauthenticated


@dataclass(frozen=True)
class DecisionNote:
    """What the app's logs and traces get for each decision."""

    action: str
    resource: EntityRef
    principal: EntityRef
    result: str
    on_behalf_of: EntityRef | None = None
    decision_qid: str | None = None
    mode: Mode | None = None
    verdict: Verdict | None = None
    fallback: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)
