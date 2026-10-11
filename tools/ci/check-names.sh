#!/usr/bin/env bash
# Fails when a name or the docs drift from one word for each thing (ADR 0074, 0079):
#   - code names with `person`, `people`, or `accessor` (a principal, or a human, is named so);
#   - "accessor" or "PEP" in the docs, outside the glossary's and the platform plan's one pointer.
# Comments, string literals, and test names (which read as sentences) can say "person".
#
#   tools/ci/check-names.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import re, subprocess, sys

def files(*patterns):
    out = subprocess.run(["git", "ls-files", *patterns], capture_output=True, text=True).stdout.split()
    skip = ("__generated__", "/migrations/", "docs/adr/", "schema.graphql", "schema.sql")
    return [f for f in out if not any(s in f for s in skip)]

bad = []
name = re.compile(r"\b\w*(person|people|accessor)\w*\b", re.I)
literal = re.compile(r'"(?:[^"\\]|\\.)*"|`[^`]*`|\'(?:[^\'\\\n]|\\.){2,}\'')
for f in files("*.rs", "*.ts", "*.tsx", "*.proto", "*.cedar", "*.cedarschema"):
    lines = open(f).read().split("\n")
    for i, line in enumerate(lines):
        code = line.split("//")[0]
        if code.strip().startswith(("*", "/*")):
            continue
        code = literal.sub('""', code)
        if re.match(r"\s*(pub )?(async )?fn \w+\(", code) and i and "test" in lines[i - 1]:
            continue
        for m in name.finditer(code):
            # In TypeScript, JSX text is prose; only a name where a name goes counts.
            if f.endswith((".ts", ".tsx")):
                before, after = code[: m.start()], code[m.end():]
                declared = re.search(r"(\.|\b(const|let|var|function|type|interface|enum)\s+)$", before)
                if not declared and not re.match(r"\s*(\(|=(?!=)|:)", after):
                    continue
            bad.append(f"{f}:{i + 1}: `{m.group(0)}` in a name: say human, humans, or principal")

for f in files("*.md", "*.mdx"):
    for i, line in enumerate(open(f).read().split("\n")):
        if re.search(r"\bPEPs?\b", line) and not ("This plan uses two standard authorization terms" in line or '"PEP"' in line):
            bad.append(f"{f}:{i + 1}: say \"enforcement point\", not PEP")
        if re.search(r"\baccessors?\b", line, re.I) and not re.search(r"\*\*Accessor\.\*\* See \*principal\*|\"accessor\"", line):
            bad.append(f"{f}:{i + 1}: say \"principal\", not accessor")

for b in bad:
    print(b)
sys.exit(1 if bad else 0)
PY
