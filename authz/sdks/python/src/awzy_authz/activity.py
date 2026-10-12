"""Recording what principals do through the app in awzy-intake (ADR 0084)."""

from __future__ import annotations

import contextlib
import threading
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

import httpx

from .types import EntityRef

#: Events queued before new ones are dropped.
_QUEUE = 10_000
#: Most events in one request; awzy-intake takes up to 1,000.
_BATCH = 500
_EVERY_S = 1.0


@dataclass(frozen=True)
class Activity:
    """One thing a principal did, in awzy-intake's app event schema. No argument values."""

    principal: EntityRef
    action: str
    on_behalf_of: EntityRef | None = None
    resource: EntityRef | None = None
    outcome: str | None = None
    #: How the resource was touched, and how many rows: ``listed`` for a plan's results.
    access: str | None = None
    count: int | None = None
    attributes: dict[str, Any] = field(default_factory=dict)
    at: datetime = field(default_factory=lambda: datetime.now(timezone.utc))


class ActivityLog:
    """Best effort: queued, sent a batch a second from a background thread, and dropped when the
    queue is full or awzy-intake can't be reached. Recording never slows a call."""

    def __init__(self, url: str, token: str, system: str, *, http: httpx.Client | None = None) -> None:
        self._endpoint = f"{url.rstrip('/')}/v1/events"
        self._token = token
        self._system = system
        self._http = http or httpx.Client()
        self._queue: list[Activity] = []
        self._lock = threading.Lock()
        self._thread: threading.Thread | None = None

    def record(self, activity: Activity) -> None:
        """Queues ``activity`` to be sent. Never waits."""
        with self._lock:
            if len(self._queue) >= _QUEUE:
                return
            self._queue.append(activity)
            if self._thread is None:
                # A daemon thread: it never keeps a script running just to send activity.
                self._thread = threading.Thread(target=self._send_forever, daemon=True)
                self._thread.start()

    def flush(self) -> None:
        """Sends what's queued now. Call before a script exits."""
        while True:
            with self._lock:
                batch, self._queue = self._queue[:_BATCH], self._queue[_BATCH:]
            if not batch:
                return
            events = [self._event(a) for a in batch]
            # Best effort: a batch awzy-intake doesn't take is dropped.
            with contextlib.suppress(httpx.HTTPError):
                self._http.post(
                    self._endpoint,
                    json={"events": events},
                    headers={"authorization": f"Bearer {self._token}"},
                    timeout=10.0,
                )

    def _send_forever(self) -> None:
        stop = threading.Event()
        while not stop.wait(_EVERY_S):
            self.flush()

    def _event(self, a: Activity) -> dict[str, Any]:
        e: dict[str, Any] = {
            "at": a.at.astimezone(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
            "system": self._system,
            "principal": a.principal.to_json(),
            "action": a.action,
            "attributes": a.attributes,
        }
        if a.on_behalf_of:
            e["on_behalf_of"] = a.on_behalf_of.to_json()
        if a.resource:
            e["resource"] = a.resource.to_json()
        if a.outcome:
            e["outcome"] = a.outcome
        if a.access:
            e["access"] = a.access
        if a.count is not None:
            e["count"] = a.count
        return e
