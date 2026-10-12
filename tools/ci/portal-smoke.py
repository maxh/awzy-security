#!/usr/bin/env python3
"""portal-smoke.py <host>...: every page of a demo's client portal answers.

Signs in to each demo install (https://<host>) as its security admin through the demo IdP, then
runs every query the deployed frontend sends (its persisted_queries.json, so always the queries
that build actually uses) and fails on any error. A page that can't load for any reason shows
up here: a service refusing the portal, a schema mismatch, a backend that's down.

Only demo installs: they run the demo IdP, so anyone may sign in as a sample person, and they're
read-only, so nothing here changes them. Queries that take input get sample values from the
synthetic company; ones with no sample are listed as skipped.
"""

import base64
import hashlib
import json
import re
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request

USER = "alice"  # Larkspur Capital's security team: an admin who can see every page.

# Inputs for the queries that need them, from the synthetic company.
SAMPLES = {
    "LookupPageSearchQuery": {"query": "alice"},
    "LookupPagePrincipalQuery": {"type": "HUMAN", "id": "alice"},
    "SecurityKeysKeysOfQuery": {"humanId": "alice"},
}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def sign_in(base):
    """An access token for USER from the demo IdP, by the authorization code flow with PKCE."""
    verifier = secrets.token_urlsafe(48)
    challenge = base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
    redirect = f"{base}/"
    query = urllib.parse.urlencode({
        "user": USER, "client_id": "awzy-client-portal", "redirect_uri": redirect,
        "response_type": "code", "code_challenge": challenge, "code_challenge_method": "S256",
    })
    opener = urllib.request.build_opener(NoRedirect)
    try:
        opener.open(f"{base}/idp/authorize/as?{query}", timeout=20)
        raise SystemExit("the demo IdP didn't redirect with a code")
    except urllib.error.HTTPError as e:
        location = e.headers.get("Location", "")
    code = urllib.parse.parse_qs(urllib.parse.urlparse(location).query).get("code", [None])[0]
    if not code:
        raise SystemExit(f"no code in the demo IdP's redirect: {location!r}")
    body = urllib.parse.urlencode({
        "grant_type": "authorization_code", "code": code, "code_verifier": verifier,
        "redirect_uri": redirect, "client_id": "awzy-client-portal",
    }).encode()
    with urllib.request.urlopen(f"{base}/idp/token", data=body, timeout=20) as res:
        return json.load(res)["access_token"]


def run(base, token, query_id, variables):
    req = urllib.request.Request(
        f"{base}/graphql",
        data=json.dumps({"query": "", "variables": variables,
                         "extensions": {"persistedQuery": {"version": 1, "sha256Hash": query_id}}}).encode(),
        headers={"authorization": f"Bearer {token}", "content-type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as res:
            return json.load(res)
    except urllib.error.HTTPError as e:
        return {"errors": [{"message": f"HTTP {e.code}: {e.read()[:300].decode(errors='replace')}"}]}


def required_variables(text):
    match = re.match(r"\s*query\s+\w+\s*\(([^)]*)\)", text)
    if not match:
        return []
    return [d.split(":")[0].strip().lstrip("$") for d in re.split(r",\s*(?=\$)", match.group(1))
            if d.strip().endswith("!") and "=" not in d]


def first_qid(value, entity):
    """The first `<entity>_…` QID anywhere in a query's result."""
    if isinstance(value, str):
        return value if value.startswith(f"{entity}_") else None
    children = value.values() if isinstance(value, dict) else value if isinstance(value, list) else []
    for child in children:
        found = first_qid(child, entity)
        if found:
            return found
    return None


def smoke(host):
    base = f"https://{host}"
    token = sign_in(base)
    with urllib.request.urlopen(f"{base}/persisted_queries.json", timeout=20) as res:
        operations = json.load(res)
    failed, skipped, ran = [], [], 0
    samples = dict(SAMPLES)
    # Queries that need an ID from another's result go last: a decision's detail, from the
    # decisions page.
    later = {"DecisionDetailQuery"}
    order = sorted(operations.items(), key=lambda o: (re.match(r"\s*\w+\s+(\w+)", o[1]).group(1) in later, o[1]))
    for query_id, text in order:
        if not text.lstrip().startswith("query"):
            continue  # mutations and subscriptions: a read-only demo refuses the one, and the other needs a socket
        name = re.match(r"\s*query\s+(\w+)", text).group(1)
        variables = samples.get(name, {})
        if any(v not in variables for v in required_variables(text)):
            skipped.append(name)
            continue
        result = run(base, token, query_id, variables)
        ran += 1
        errors = result.get("errors") or []
        if name == "DecisionsPageQuery" and (decision := first_qid(result.get("data"), "decision")):
            samples["DecisionDetailQuery"] = {"id": decision}
        if errors:
            failed.append((name, errors[0].get("message", "")[:300]))
    print(f"{host}: {ran} page queries ran, {len(failed)} failed, {len(skipped)} skipped (no sample input: {', '.join(skipped)})")
    for name, message in failed:
        print(f"  FAILED {name}: {message}")
    return not failed


if __name__ == "__main__":
    hosts = sys.argv[1:]
    if not hosts:
        raise SystemExit(__doc__)
    results = [smoke(h) for h in hosts]
    sys.exit(0 if all(results) else 1)
