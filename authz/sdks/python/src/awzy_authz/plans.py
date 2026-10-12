"""Query plans (ADR 0113): which records of a type a principal may act on, as a filter for the
app's own query, and the reference Postgres mapper."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from typing import Any, Literal

#: A filter node, as awzy sends it: ``{"op": "compare", "attr": ["tenant"], "cmp": "eq", "value": "acme"}``.
Filter = dict[str, Any]

PlanOutcome = Literal["always_allowed", "always_denied", "challenge", "conditional"]

TRUE: Filter = {"op": "true"}
FALSE: Filter = {"op": "false"}

_CMPS = {"eq": "=", "ne": "<>", "lt": "<", "le": "<=", "gt": ">", "ge": ">="}


def _is_entity(v: Any) -> bool:
    return isinstance(v, dict) and isinstance(v.get("type"), str) and isinstance(v.get("id"), str)


def _is_literal(v: Any) -> bool:
    return isinstance(v, (bool, int, str)) or _is_entity(v)


def _is_path(v: Any) -> bool:
    return isinstance(v, list) and len(v) > 0 and all(isinstance(p, str) for p in v)


def parse_filter(v: Any) -> Filter:
    """Reads a filter from awzy's JSON. Anything it doesn't recognize, as from a newer awzy,
    becomes ``unmappable``, so the rows are checked one by one rather than shown."""
    unmappable: Filter = {"op": "unmappable", "cedar": json.dumps(v)}
    if not isinstance(v, dict):
        return unmappable
    op = v.get("op")
    if op in ("true", "false"):
        return {"op": op}
    if op in ("and", "or"):
        args = v.get("args")
        return {"op": op, "args": [parse_filter(a) for a in args]} if isinstance(args, list) else unmappable
    if op == "not":
        return {"op": "not", "arg": parse_filter(v.get("arg"))}
    attr = v.get("attr")
    if op == "compare" and _is_path(attr) and v.get("cmp") in _CMPS and _is_literal(v.get("value")):
        return {"op": op, "attr": attr, "cmp": v["cmp"], "value": v["value"]}
    if op == "has" and _is_path(attr):
        return {"op": op, "attr": attr}
    if op == "contains" and _is_path(attr) and _is_literal(v.get("value")):
        return {"op": op, "attr": attr, "value": v["value"]}
    values = v.get("values")
    if (
        op in ("contains_any", "contains_all")
        and _is_path(attr)
        and isinstance(values, list)
        and all(_is_literal(x) for x in values)
    ):
        return {"op": op, "attr": attr, "values": values}
    if op == "like" and _is_path(attr) and isinstance(v.get("pattern"), str):
        return {"op": op, "attr": attr, "pattern": v["pattern"]}
    if op == "in" and _is_entity(v.get("ancestor")):
        return {"op": op, "ancestor": v["ancestor"]}
    if op == "is" and _is_entity(v.get("id")):
        return {"op": op, "id": v["id"]}
    if op == "unknown" and isinstance(v.get("name"), str):
        return {"op": op, "name": v["name"]}
    return unmappable


def needs_row_checks(f: Filter) -> bool:
    """Whether some row has to be checked one by one."""
    op = f.get("op")
    if op in ("unknown", "unmappable"):
        return True
    if op in ("and", "or"):
        return any(needs_row_checks(a) for a in f.get("args", []))
    if op == "not":
        return needs_row_checks(f.get("arg", TRUE))
    return False


@dataclass(frozen=True)
class Plan:
    """awzy's answer to a list or search: what to show, as a filter, and what it rests on."""

    qid: str
    outcome: PlanOutcome
    filter: Filter
    #: The filter to apply now, with the mode applied: every row in monitor, no row while a
    #: challenge is pending, otherwise the plan's own.
    apply: Filter
    mode: Literal["monitor", "enforce"]
    challenges: list[str] = field(default_factory=list)
    challenge_could_widen: bool = False
    needs_row_checks: bool = False
    attributes: list[str] = field(default_factory=list)
    max_age_secs: int = 0
    pinned: bool = False


@dataclass(frozen=True)
class SqlMapping:
    """How a type's attributes sit in a Postgres table.

    ``id`` is the row's ID column, qualified with its table (``customers.id``) so the ancestor
    join's subquery can't mistake it for a column of its own. ``columns`` maps each attribute path,
    joined with dots, to its column. ``ancestors`` is ``(table, row column, ancestor column)``: a
    table holding each row's ancestors, for ``in``.
    """

    id: str
    columns: dict[str, str]
    ancestors: tuple[str, str, str] | None = None


@dataclass(frozen=True)
class Sql:
    #: A boolean expression for a ``WHERE`` clause, with ``%s`` placeholders for ``params``.
    where: str
    params: list[Any]
    #: Some of the filter couldn't be mapped: ``where`` is a superset, so check each row it
    #: returns one by one before showing it.
    needs_row_checks: bool
    unmapped: list[str]


def like_pattern(cedar: str) -> str:
    """Cedar's ``like`` pattern as SQL's: ``*`` is ``%``, ``\\*`` a literal star, and SQL's own
    ``%``, ``_``, and ``\\`` escaped."""
    out = []
    i = 0
    while i < len(cedar):
        c = cedar[i]
        if c == "\\" and cedar[i + 1 : i + 2] == "*":
            out.append("*")
            i += 2
            continue
        if c == "*":
            out.append("%")
        elif c in ("%", "_", "\\"):
            out.append("\\" + c)
        else:
            out.append(c)
        i += 1
    return "".join(out)


_NAME = re.compile(r"^[a-z_][a-z0-9_]*(\.[a-z_][a-z0-9_]*)?$")


def _quote(name: str) -> str:
    return name if _NAME.match(name) else '"' + name.replace('"', '""') + '"'


def to_postgres(f: Filter, mapping: SqlMapping) -> Sql:
    """A plan's filter as a Postgres ``WHERE`` clause, with psycopg-style ``%s`` parameters. What
    it can't map is replaced by whatever keeps the clause a superset of the rows the filter
    matches (``TRUE`` where it allows rows, ``FALSE`` under a ``not``), and reported."""
    params: list[Any] = []
    unmapped: list[str] = []

    def value(lit: Any) -> Any:
        return lit["id"] if isinstance(lit, dict) else lit

    def param(v: Any) -> str:
        params.append(v)
        return "%s"

    def walk(node: Filter, positive: bool) -> str:
        def cant(why: str) -> str:
            unmapped.append(why)
            return "TRUE" if positive else "FALSE"

        def col(attr: list[str]) -> str | None:
            c = mapping.columns.get(".".join(attr))
            return None if c is None else _quote(c)

        op = node.get("op")
        if op == "true":
            return "TRUE"
        if op == "false":
            return "FALSE"
        if op in ("and", "or"):
            args = node.get("args", [])
            if not args:
                return "TRUE" if op == "and" else "FALSE"
            return "(" + f" {op.upper()} ".join(walk(a, positive) for a in args) + ")"
        if op == "not":
            return "NOT " + walk(node.get("arg", TRUE), not positive)
        if op == "is":
            return f"{_quote(mapping.id)} = {param(value(node['id']))}"
        if op == "in":
            if not mapping.ancestors:
                return cant("no ancestors table for in")
            table, row, ancestor = (_quote(n) for n in mapping.ancestors)
            p = value(node["ancestor"])
            row_id = _quote(mapping.id)
            return (
                f"({row_id} = {param(p)} OR EXISTS (SELECT 1 FROM {table} WHERE {table}.{row} = {row_id} "
                f"AND {table}.{ancestor} = {param(p)}))"
            )
        if op == "unknown":
            return cant(f"{node.get('name')} is worked out by awzy for each row")
        if op == "unmappable":
            return cant(f"Cedar the filter can't express: {node.get('cedar')}")
        attr = node.get("attr")
        c = col(attr) if isinstance(attr, list) else None
        if c is None:
            return cant(f"no column for {'.'.join(attr) if isinstance(attr, list) else attr}")
        if op == "compare":
            return f"{c} {_CMPS[node['cmp']]} {param(value(node['value']))}"
        if op == "has":
            return f"{c} IS NOT NULL"
        if op == "contains":
            return f"{param(value(node['value']))} = ANY({c})"
        if op == "contains_any":
            return f"{c} && {param([value(v) for v in node['values']])}"
        if op == "contains_all":
            return f"{c} @> {param([value(v) for v in node['values']])}"
        if op == "like":
            return f"{c} LIKE {param(like_pattern(node['pattern']))}"
        return cant(f"a filter node it doesn't know: {json.dumps(node)}")

    where = walk(f, True)
    return Sql(where, params, bool(unmapped), unmapped)
