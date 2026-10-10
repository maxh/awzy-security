-- People's security keys (WebAuthn ES256 credentials, ADR 0059). A removed key keeps its row,
-- with who removed it, as the audit trail.
CREATE TABLE security_keys (
    id            uuid PRIMARY KEY,
    person_id     text NOT NULL,
    credential_id bytea NOT NULL UNIQUE,
    -- SEC1 uncompressed P-256 point.
    public_key    bytea NOT NULL,
    sign_count    bigint NOT NULL,
    name          text NOT NULL,
    -- The existing key that authorized adding this one; null for a person's first key.
    added_with    uuid REFERENCES security_keys (id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    last_used_at  timestamptz,
    removed_at    timestamptz,
    -- The person's own key, or an admin who reset it.
    removed_with  uuid REFERENCES security_keys (id),
    removed_by    text
);

CREATE INDEX security_keys_active_by_person ON security_keys (person_id) WHERE removed_at IS NULL;

-- One WebAuthn ceremony: the random challenge awzy issued and what it's for. Single use, short-lived.
CREATE TABLE webauthn_ceremonies (
    id            uuid PRIMARY KEY,
    person_id     text NOT NULL,
    purpose       text NOT NULL CHECK (purpose IN ('register', 'key_change', 'verify')),
    challenge     bytea NOT NULL,
    -- For 'verify': the awzy challenge it answers.
    challenge_id  uuid REFERENCES challenges (id),
    -- For 'register' by someone who already has a key: the key that authorized it.
    authorized_by uuid REFERENCES security_keys (id),
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL,
    used_at       timestamptz
);
