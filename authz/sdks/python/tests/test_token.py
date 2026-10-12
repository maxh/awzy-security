from __future__ import annotations

import time
from typing import Any

import jwt
import pytest
from cryptography.hazmat.primitives.asymmetric import ec

from awzy_authz import EntityRef, Principal, TokenError, TokenVerifier, TrustedApp

IDP_KEY = ec.generate_private_key(ec.SECP256R1())
APP_KEY = ec.generate_private_key(ec.SECP256R1())


def token(claims: dict[str, Any], *, key: Any = IDP_KEY, iss: str = "https://idp.test", aud: str = "awzy") -> str:
    body = {"iss": iss, "aud": aud, "exp": int(time.time()) + 300, **claims}
    return "Bearer " + jwt.encode(body, key, algorithm="ES256")


def verifier(**extra: Any) -> TokenVerifier:
    return TokenVerifier("https://idp.test", "awzy", lambda _t: IDP_KEY.public_key(), **extra)


def test_a_plain_token_is_a_person_and_act_is_an_agent_acting_for_them() -> None:
    v = verifier()
    assert v.principal(token({"sub": "alice"})) == Principal(EntityRef("Human", "alice"))
    assert v.principal(token({"sub": "alice", "act": {"sub": "sales-genie"}, "awzy_human_present": False})) == (
        Principal(EntityRef("ServiceAccount", "sales-genie"), EntityRef("Human", "alice"), False)
    )


def test_the_work_item_an_agents_token_names_is_carried() -> None:
    v = verifier()
    p = v.principal(token({"sub": "alice", "act": {"sub": "sales-genie"}, "awzy_work_item_id": "jira:SALES-12"}))
    assert p.work_item_id == "jira:SALES-12"
    assert v.principal(token({"sub": "alice", "awzy_work_item_id": "  "})).work_item_id is None


def test_an_applications_own_token_is_known_by_its_client_credentials_claims() -> None:
    v = verifier(subject_claim="email")
    for claims in [
        {"sub": "nightly-sync", "client_id": "nightly-sync"},
        {"sub": "0oa9app", "cid": "0oa9app"},
        {"sub": "5f1c-app", "idtyp": "app"},
    ]:
        assert v.principal(token(claims)).principal.type == "ServiceAccount"
    assert v.principal(token({"sub": "00u1", "email": "bob@acme.com"})).principal == EntityRef("Human", "bob@acme.com")
    with pytest.raises(TokenError, match="email"):
        v.principal(token({"sub": "00u1"}))


def test_a_token_for_another_audience_or_issuer_or_none_is_refused() -> None:
    v = verifier()
    for bad in [token({"sub": "alice"}, aud="other"), token({"sub": "alice"}, iss="https://evil.test"), "Bearer x"]:
        with pytest.raises(TokenError):
            v.principal(bad)
    with pytest.raises(TokenError, match="missing"):
        v.principal(None)


def test_an_app_signs_only_for_its_own_agents() -> None:
    app = TrustedApp("acme-crm", "https://crm.acme.example", lambda _t: APP_KEY.public_key(), "awzy-agents")
    v = verifier(trusted_apps=[app])

    def crm(claims: dict[str, Any]) -> str:
        return token(claims, key=APP_KEY, iss="https://crm.acme.example", aud="awzy-agents")

    assert v.principal(crm({"sub": "acme-crm.renewals-bot"})) == Principal(
        EntityRef("ServiceAccount", "acme-crm.renewals-bot")
    )
    assert v.principal(crm({"sub": "dana", "act": {"sub": "acme-crm.assistant"}})) == Principal(
        EntityRef("ServiceAccount", "acme-crm.assistant"), EntityRef("Human", "dana")
    )
    for claims in [{"sub": "dana"}, {"sub": "sales-genie"}, {"sub": "acme-crm"}, {"sub": "x", "act": {"sub": "bot"}}]:
        with pytest.raises(TokenError, match="signs tokens only for"):
            v.principal(crm(claims))
    with pytest.raises(TokenError):
        v.principal(token({"sub": "acme-crm.bot"}, key=APP_KEY))


def test_a_jwks_url_must_be_https() -> None:
    with pytest.raises(TokenError, match="https"):
        TokenVerifier("https://idp.test", "awzy", "http://idp.test/jwks")
