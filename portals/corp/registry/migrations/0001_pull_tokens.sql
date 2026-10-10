-- Pull tokens for registry.awzysecurity.com (ADR 0028), one per install. Lives in its own
-- schema ("registry") in the corp portal's database.
CREATE TABLE pull_tokens (
    install_id UUID PRIMARY KEY,
    -- SHA-256 of the token; the token itself is shown once and never stored.
    token_sha256 BYTEA NOT NULL UNIQUE,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
