"""A PDP on a local port that answers as each conformance step says: a response, a dropped
connection (``down``), or nothing for longer than any budget (``hang``)."""

from __future__ import annotations

import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any


class FakePdp:
    def __init__(self) -> None:
        self.answers: dict[str, Any] = {}
        self.sent: list[Any] = []
        fake = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:  # noqa: N802 (http.server's name)
                body = self.rfile.read(int(self.headers.get("content-length", "0")))
                name = (
                    "evaluation"
                    if self.path == "/v1/access/evaluation"
                    else ("challenge" if self.path == "/v1/access/challenge" else "plan")
                )
                if name == "evaluation":
                    fake.sent.append(json.loads(body))
                answer = fake.answers.get(name, "down")
                if answer == "down":
                    self.close_connection = True
                    return
                if answer == "hang":
                    time.sleep(2)
                    self.close_connection = True
                    return
                payload = json.dumps(answer.get("body")).encode()
                self.send_response(answer.get("status", 200))
                self.send_header("content-type", "application/json")
                self.send_header("content-length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, *args: Any) -> None:
                pass

        self._server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self._server.daemon_threads = True
        self.url = f"http://127.0.0.1:{self._server.server_address[1]}"
        threading.Thread(target=self._server.serve_forever, daemon=True).start()

    def close(self) -> None:
        self._server.shutdown()
        self._server.server_close()
