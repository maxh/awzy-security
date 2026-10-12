"""Validating callers' tokens, as awzy-gateway does (ADR 0045, rule 1)."""

from __future__ import annotations

import ssl
from collections.abc import Callable, Sequence
from dataclasses import dataclass, replace
from typing import Any

import jwt

from .types import EntityRef, Principal

ALGORITHMS = ["RS256", "ES256"]

#: Finds the key a token was signed with: a JWKS URL's client, or a function the app supplies.
KeyFinder = Callable[[str], Any]


class TokenError(Exception):
    """The caller's token isn't valid."""


@dataclass(frozen=True)
class TrustedApp:
    """An app that signs tokens for the service accounts whose ID starts with its name and a dot."""

    name: str
    issuer: str
    #: Its JWKS URL (https), or a function from a token to its signing key.
    keys: str | KeyFinder
    audience: str


def _key_finder(keys: str | KeyFinder, ca_file: str | None) -> KeyFinder:
    if callable(keys):
        return keys
    # Keys fetched over plain HTTP could be swapped in transit and used to forge tokens.
    if not keys.startswith("https://"):
        raise TokenError(f"a JWKS URL must be https://, not {keys}")
    context = ssl.create_default_context(cafile=ca_file) if ca_file else None
    client = jwt.PyJWKClient(keys, ssl_context=context) if context else jwt.PyJWKClient(keys)
    return lambda token: client.get_signing_key_from_jwt(token).key


def _is_client_credentials(claims: dict[str, Any]) -> bool:
    """A token an application got for itself: RFC 9068's ``client_id``, Okta's ``cid``, Entra's ``idtyp``."""
    sub = claims.get("sub")
    return claims.get("client_id") == sub or claims.get("cid") == sub or claims.get("idtyp") == "app"


class TokenVerifier:
    """Validates callers' tokens (signature, issuer, audience, expiry) and builds the principal
    from their claims only.

    ``subject_claim`` names the claim that identifies a person: their user name as the IdP
    provisions them. Default ``sub``; with Okta or Entra usually ``email``. ``service_accounts``
    lists service accounts whose own tokens carry no client-credentials marker. ``trusted_apps``
    are apps an admin trusts to sign tokens for their own agents.
    """

    def __init__(
        self,
        issuer: str,
        audience: str,
        keys: str | KeyFinder,
        *,
        subject_claim: str | None = None,
        service_accounts: Sequence[str] = (),
        trusted_apps: Sequence[TrustedApp] = (),
        ca_file: str | None = None,
    ) -> None:
        self._issuer = issuer
        self._audience = audience
        self._keys = _key_finder(keys, ca_file)
        self._subject_claim = subject_claim if subject_claim and subject_claim != "sub" else None
        self._service_accounts = frozenset(service_accounts)
        self._apps = {a.issuer: (a, _key_finder(a.keys, ca_file)) for a in trusted_apps}

    def principal(self, authorization: str | None) -> Principal:
        """The principal an ``Authorization`` header's bearer token names, or a TokenError."""
        if not authorization or not authorization.startswith("Bearer "):
            raise TokenError("missing bearer token")
        token = authorization[len("Bearer ") :]
        try:
            iss = jwt.decode(token, options={"verify_signature": False}).get("iss")
        except jwt.PyJWTError as e:
            raise TokenError("not a JWT") from e
        if iss != self._issuer and iss in self._apps:
            app, keys = self._apps[iss]
            return self._app_principal(self._verify(token, keys, app.issuer, app.audience), app)
        claims = self._verify(token, self._keys, self._issuer, self._audience)
        sub = claims["sub"]
        act = claims.get("act")
        if isinstance(act, dict) and isinstance(act.get("sub"), str):
            return self._with_awzy_claims(
                claims,
                Principal(EntityRef("ServiceAccount", act["sub"]), EntityRef("Human", self._human(claims))),
            )
        if _is_client_credentials(claims) or sub in self._service_accounts:
            return self._with_awzy_claims(claims, Principal(EntityRef("ServiceAccount", sub)))
        return self._with_awzy_claims(claims, Principal(EntityRef("Human", self._human(claims))))

    def _verify(self, token: str, keys: KeyFinder, issuer: str, audience: str) -> dict[str, Any]:
        try:
            claims: dict[str, Any] = jwt.decode(
                token,
                keys(token),
                algorithms=ALGORITHMS,
                issuer=issuer,
                audience=audience,
                options={"require": ["exp", "iss", "aud", "sub"]},
            )
        except (jwt.PyJWTError, KeyError, ValueError) as e:
            raise TokenError(f"invalid token: {e}") from e
        return claims

    def _human(self, claims: dict[str, Any]) -> str:
        if not self._subject_claim:
            return str(claims["sub"])
        value = claims.get(self._subject_claim)
        if not isinstance(value, str) or not value:
            raise TokenError(f'the token has no "{self._subject_claim}" claim to identify the person')
        return value

    def _app_principal(self, claims: dict[str, Any], app: TrustedApp) -> Principal:
        """An app signs only for its own agents, on their own or acting for a person: never a person."""
        act = claims.get("act")
        acting = act.get("sub") if isinstance(act, dict) else None
        agent = acting if isinstance(acting, str) else str(claims["sub"])
        prefix = f"{app.name}."
        if not agent.startswith(prefix) or agent == prefix:
            raise TokenError(f'{app.name} signs tokens only for service accounts under "{prefix}", not "{agent}"')
        on_behalf_of = EntityRef("Human", str(claims["sub"])) if isinstance(acting, str) else None
        return self._with_awzy_claims(claims, Principal(EntityRef("ServiceAccount", agent), on_behalf_of))

    @staticmethod
    def _with_awzy_claims(claims: dict[str, Any], p: Principal) -> Principal:
        """What awzy's own claims add: whether a person is present, and the work item."""
        present = claims.get("awzy_human_present")
        item = claims.get("awzy_work_item_id")
        return replace(
            p,
            human_present=present if isinstance(present, bool) else p.human_present,
            work_item_id=item.strip() or None if isinstance(item, str) else p.work_item_id,
        )
