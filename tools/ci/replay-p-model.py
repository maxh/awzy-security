#!/usr/bin/env python3
"""replay-p-model.py: checks a run's logs against a P model's properties (ADR 0130).

Services log the events a model names at debug level under the `awzy_model` target
(`RUST_LOG=info,awzy_model=debug AWZY_LOG_FORMAT=json`). This reads those JSON lines from any
number of files, in time order, and writes them as a P machine that announces each in turn. It
then runs the replay's tests: the model's own spec machines, observing what the services did
instead of what the model's machines do. A run that broke a property fails with the line that
broke it.

    tools/ci/replay-p-model.py <replay dir> <log>... [--timing Name=value]...
    tools/ci/replay-p-model.py --self-test [<replay dir>...]

A replay dir sits in a model's folder and holds:
  replay.json  `model`: the model's files the replay needs (its events and specs), relative to
               the replay dir; `increasing`: events that keep a line only when the named field is
               higher than on any before it, such as a version that more than one service logs;
               `since`: fields that name another event's field, such as a decision's list version
               naming a stored version, as "eDecision.basis": "eStoreChanged.version"; a line
               whose value is below every one logged so far comes from before the logs began, and
               is skipped;
               `fixtures`: log files for --self-test, each with the test it must break, or null
               when every test must pass.
  timings.p    the model's timing functions, with real values in milliseconds.
  tests.p      the test cases, with `main = Replay` and `Replay, Follower` as their machines.

Each logged field fills the event's payload field of the same name, by the type events.p declares:
an int from a number, or from a string, which stands for the same int wherever it appears (a
principal); a machine from a string, one stand-in machine for each (a PDP's instance); a set[int]
from a JSON list. A payload field `at` that the line doesn't have is the line's time, in
milliseconds since the first line. With no replay dir, --self-test runs every one in the repository.
Needs the P toolchain, as tools/ci/check-p-models.sh does.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

TARGET = "awzy_model"
# A generated function per this many events, so none grows too large for the C# compiler.
CHUNK = 200


def parse_events(text):
    """The events a model declares, each to its payload: a list of (name, type), a bare type, or
    None. Type aliases are resolved."""
    text = re.sub(r"//[^\n]*", "", text)
    aliases = dict(re.findall(r"\btype\s+(\w+)\s*=\s*([^;]+);", text))

    def resolve(t):
        t = t.strip()
        while t in aliases:
            t = aliases[t].strip()
        return t

    events = {}
    for name, payload in re.findall(r"\bevent\s+(\w+)\s*(?::\s*([^;]+))?;", text):
        payload = (payload or "").strip()
        if not payload:
            events[name] = None
        elif payload.startswith("("):
            fields = []
            for part in split_top(payload[1:-1]):
                field, _, t = part.partition(":")
                fields.append((field.strip(), resolve(t)))
            events[name] = fields
        else:
            events[name] = resolve(payload)
    return events


def split_top(s):
    """Splits on the commas outside brackets."""
    parts, depth, cur = [], 0, ""
    for ch in s:
        if ch in "([":
            depth += 1
        elif ch in ")]":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        parts.append(cur)
    return parts


def parse_time(stamp):
    """Milliseconds since the epoch, from tracing's RFC 3339 timestamp (up to nanoseconds)."""
    m = re.fullmatch(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(?:\.(\d+))?(Z|[+-]\d\d:\d\d)", stamp)
    if not m:
        raise ValueError(f"unreadable timestamp {stamp!r}")
    zone = "+00:00" if m[3] == "Z" else m[3]
    whole = datetime.fromisoformat(m[1] + zone).astimezone(timezone.utc)
    fraction = (m[2] or "0")[:3].ljust(3, "0")
    return int(whole.timestamp()) * 1000 + int(fraction)


def read_lines(paths):
    """The model's events from the logs, in time order. A line may carry a prefix, as docker
    compose adds one; anything that isn't one of the model's lines is skipped."""
    found = []
    for path in paths:
        with open(path, encoding="utf-8") as f:
            for n, raw in enumerate(f, 1):
                start = raw.find("{")
                if start < 0:
                    continue
                try:
                    line = json.loads(raw[start:])
                except json.JSONDecodeError:
                    continue
                if not isinstance(line, dict) or line.get("target") != TARGET:
                    continue
                fields = line.get("fields") or {}
                if "model_event" not in fields:
                    continue
                found.append((parse_time(line["timestamp"]), f"{path}:{n}", fields))
    found.sort(key=lambda e: e[0])
    return found


class Trace:
    """Writes the logged events as the `Replay` machine's announcements."""

    def __init__(self, events, increasing, since):
        self.events = events
        self.increasing = increasing
        self.since = {tuple(k.split(".")): tuple(v.split(".")) for k, v in since.items()}
        self.lowest = {}
        self.skipped = 0
        self.highest = {}
        self.ints = {}
        self.machines = {}
        self.steps = []
        self.sets = 0

    def add(self, at, where, fields):
        name = fields["model_event"]
        if name not in self.events:
            raise ValueError(f"{where}: the model declares no event {name}")
        for (event, field), ref in self.since.items():
            if event == name and (ref not in self.lowest or fields.get(field) < self.lowest[ref]):
                self.skipped += 1
                return
        for field, value in fields.items():
            ref = (name, field)
            if isinstance(value, int) and (ref not in self.lowest or value < self.lowest[ref]):
                self.lowest[ref] = value
        key = self.increasing.get(name)
        if key is not None:
            value = fields.get(key)
            if name in self.highest and value <= self.highest[name]:
                return
            self.highest[name] = value
        payload = self.events[name]
        setup = []
        if payload is None:
            self.steps.append(([], f"announce {name};", where))
            return
        if isinstance(payload, str):
            expr = self.value(payload, fields.get("payload"), where, setup)
        else:
            parts = []
            for field, t in payload:
                if field not in fields and field == "at":
                    value = at
                elif field not in fields:
                    raise ValueError(f"{where}: {name} has no field {field}")
                else:
                    value = fields[field]
                parts.append(f"{field} = {self.value(t, value, where, setup)}")
            expr = "(" + ", ".join(parts) + ("," if len(parts) == 1 else "") + ")"
        self.steps.append((setup, f"announce {name}, {expr};", where))

    def value(self, t, v, where, setup):
        if t == "int":
            if isinstance(v, bool) or not isinstance(v, (int, str)):
                raise ValueError(f"{where}: {v!r} isn't an int")
            return str(v) if isinstance(v, int) else str(self.ints.setdefault(v, len(self.ints)))
        if t == "bool":
            return "true" if v else "false"
        if t == "machine":
            return f"m[{self.machines.setdefault(str(v), len(self.machines))}]"
        m = re.fullmatch(r"set\[(.+)\]", t)
        if m:
            items = json.loads(v) if isinstance(v, str) else v
            var = f"s{self.sets}"
            self.sets += 1
            setup.append((var, t, [self.value(m[1], i, where, setup) for i in items]))
            return var
        raise ValueError(f"{where}: the replay can't fill a {t}")

    def explain(self, message):
        """A failure in the logs' own names: the spec's file without the temp path, each stand-in
        machine as the instance it stands for (P numbers machines in the order they're made,
        `Replay` first), and the strings that became ints."""
        message = re.sub(r"(?:\.\./)+\S*?/([^/\s]+\.p:)", r"\1", message)
        names = {i + 2: name for name, i in self.machines.items()}
        message = re.sub(r"Follower\((\d+)\)", lambda m: names.get(int(m[1]), m[0]), message)
        if self.ints:
            message += "; ints: " + ", ".join(f"{i} = {name}" for name, i in self.ints.items())
        return message

    def write(self):
        out = [
            "// Generated by tools/ci/replay-p-model.py from a run's logs. Each step is a logged line.",
            "machine Follower { start state Idle {} }",
            "machine Replay {",
            "  var m: seq[machine];",
            "  start state Replaying {",
            "    entry {",
        ]
        out += [f"      m += ({i}, new Follower());" for i in range(len(self.machines))]
        chunks = [self.steps[i : i + CHUNK] for i in range(0, len(self.steps), CHUNK)]
        out += [f"      step{i}();" for i in range(len(chunks))]
        out += ["    }", "  }"]
        for i, chunk in enumerate(chunks):
            out.append(f"  fun step{i}() {{")
            out += [f"    var {var}: {t};" for setup, _, _ in chunk for var, t, _ in setup]
            for setup, announce, where in chunk:
                out.append(f"    // {where}")
                out += [f"    {var} += ({item});" for var, _, items in setup for item in items]
                out.append(f"    {announce}")
            out.append("  }")
        out.append("}")
        return "\n".join(out) + "\n"


def p_tool():
    return os.environ.get("AWZY_P", "p")


def run(replay_dir, logs, timings=None):
    """Replays `logs` against the model; returns {test: the assertion it broke, or None}."""
    replay_dir = Path(replay_dir).resolve()
    config = json.loads((replay_dir / "replay.json").read_text())
    model = [(replay_dir / f).resolve() for f in config["model"]]
    events = {}
    for f in model:
        events.update(parse_events(f.read_text()))
    trace = Trace(events, config.get("increasing", {}), config.get("since", {}))
    lines = read_lines(logs)
    if not lines:
        raise ValueError(f"no {TARGET} lines in {', '.join(map(str, logs))}")
    first = lines[0][0]
    for at, where, fields in lines:
        trace.add(at - first, where, fields)

    timing_text = (replay_dir / "timings.p").read_text()
    for name, value in (timings or {}).items():
        timing_text, n = re.subn(
            rf"fun {re.escape(name)}\(\): int {{ return -?\d+; }}", f"fun {name}(): int {{ return {value}; }}", timing_text
        )
        if n != 1:
            raise ValueError(f"timings.p has no `fun {name}(): int`")
    tests_text = (replay_dir / "tests.p").read_text()
    tests = re.findall(r"^test (\w+)", tests_text, re.M)

    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        (tmp / "timings.p").write_text(timing_text)
        (tmp / "tests.p").write_text(tests_text)
        (tmp / "trace.p").write_text(trace.write())
        files = [*model, tmp / "timings.p", tmp / "tests.p", tmp / "trace.p"]
        (tmp / "Replay.pproj").write_text(
            "<Project>\n  <ProjectName>Replay</ProjectName>\n  <InputFiles>\n"
            + "".join(f"    <PFile>{f}</PFile>\n" for f in files)
            + "  </InputFiles>\n  <OutputDir>./PGenerated/</OutputDir>\n</Project>\n"
        )
        compiled = subprocess.run(
            [p_tool(), "compile", "--pproj", "Replay.pproj"], cwd=tmp, capture_output=True, text=True, stdin=subprocess.DEVNULL
        )
        if compiled.returncode != 0 or "Compilation succeeded" not in compiled.stdout:
            raise RuntimeError(f"the replay didn't compile:\n{compiled.stdout}{compiled.stderr}")
        results = {}
        for tc in tests:
            out = tmp / f"out-{tc}"
            # The replay has no choices to explore: one schedule is the whole run.
            checked = subprocess.run(
                [p_tool(), "check", "-tc", tc, "-i", "1", "-o", str(out)],
                cwd=tmp,
                capture_output=True,
                text=True,
                stdin=subprocess.DEVNULL,
            )
            broke = None
            for report in out.rglob("*_0_0.txt"):
                text = report.read_text()
                m = re.search(r"Assertion Failed.*", text) or re.search(r"<ErrorLog>.*", text)
                broke = m[0] if m else "a failure"
                # The last logged line the replay reached is the one that broke it.
                where = re.findall(r"// (\S+:\d+)", (tmp / "trace.p").read_text())
                step = len(re.findall(r"<AnnounceLog>", text))
                if 0 < step <= len(where):
                    broke += f" (at {where[step - 1]})"
                broke = trace.explain(broke)
            if broke is None and checked.returncode != 0:
                raise RuntimeError(f"{tc} didn't run:\n{checked.stdout}{checked.stderr}")
            results[tc] = broke
        if trace.skipped:
            print(f"skipped {trace.skipped} line{'' if trace.skipped == 1 else 's'} from before the logs began")
        return results


def self_test(dirs):
    """Each fixture breaks exactly the test it names, or none."""
    failed = False
    for d in dirs:
        config = json.loads((Path(d) / "replay.json").read_text())
        for fixture, expected in config.get("fixtures", {}).items():
            results = run(d, [Path(d) / fixture])
            broken = sorted(tc for tc, b in results.items() if b)
            want = [expected] if expected else []
            if broken == want:
                print(f"ok: {d}/{fixture}: " + (f"breaks {expected}" if expected else "passes every test"))
            else:
                failed = True
                print(f"::error file={d}/{fixture}::expected {want or 'no failures'}, got {broken or 'none'}")
                for tc, b in results.items():
                    if b:
                        print(f"  {tc}: {b}")
    return failed


def main(argv):
    if argv and argv[0] == "--self-test":
        root = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True, check=True).stdout.strip()
        dirs = argv[1:] or sorted(
            str(Path(f).parent)
            for f in subprocess.run(
                ["git", "ls-files", "--cached", "--others", "--exclude-standard", "*/replay/replay.json"],
                cwd=root,
                capture_output=True,
                text=True,
                check=True,
            ).stdout.split()
        )
        os.chdir(root)
        return 1 if self_test(dirs) else 0
    timings, rest = {}, []
    it = iter(argv)
    for a in it:
        if a == "--timing":
            name, _, value = next(it, "").partition("=")
            timings[name] = int(value)
        else:
            rest.append(a)
    if len(rest) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    results = run(rest[0], rest[1:], timings)
    for tc, broke in results.items():
        print(f"{'FAIL' if broke else 'ok'}: {tc}" + (f": {broke}" if broke else ""))
    return 1 if any(results.values()) else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
