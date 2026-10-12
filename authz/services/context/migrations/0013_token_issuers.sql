-- Apps an admin trusts to sign tokens for their own agents, the service accounts under
-- `<name>.` (ADR 0110). In the principals snapshot, so changing one bumps its version.
CREATE TABLE token_issuers (
    name       TEXT PRIMARY KEY,
    issuer     TEXT NOT NULL UNIQUE,
    jwks_url   TEXT NOT NULL,
    audience   TEXT NOT NULL,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
