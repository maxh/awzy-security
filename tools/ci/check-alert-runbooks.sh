#!/usr/bin/env bash
# Fails when an alert in alerts.yaml has no runbook_url, or its runbook_url names a docs page or a
# section that doesn't exist, so whoever is paged always lands on what to do.
#
#   tools/ci/check-alert-runbooks.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import re, sys

site = "https://docs.awzysecurity.com/"
docs = "docs/site/src/content/docs/"

def slug(heading):
    # Starlight's heading IDs (github-slugger): lowercase, punctuation dropped, spaces to hyphens.
    return re.sub(r"[^\w\- ]", "", heading.strip().lower()).replace(" ", "-")

bad = []
alert = None
urls = {}
for line in open("deploy/observability/prometheus/alerts.yaml"):
    if m := re.match(r"\s*- alert: (\w+)", line):
        alert = m.group(1)
        urls[alert] = None
    elif alert and (m := re.match(r'\s+runbook_url: "?([^"\s]+)"?', line)):
        urls[alert] = m.group(1)

for alert, url in urls.items():
    if not url:
        bad.append(f"{alert}: no runbook_url")
        continue
    m = re.fullmatch(re.escape(site) + r"([\w/-]+?)/#([\w-]+)", url)
    if not m:
        bad.append(f"{alert}: runbook_url {url} isn't a section of a page on {site}")
        continue
    page, anchor = m.groups()
    try:
        text = open(f"{docs}{page}.md").read()
    except FileNotFoundError:
        bad.append(f"{alert}: no docs page {docs}{page}.md")
        continue
    if anchor not in {slug(h) for h in re.findall(r"^#{2,4} (.+)$", text, re.M)}:
        bad.append(f"{alert}: {docs}{page}.md has no section #{anchor}")

for b in bad:
    print(f"::error::{b}", file=sys.stderr)
sys.exit(1 if bad else 0)
PY
