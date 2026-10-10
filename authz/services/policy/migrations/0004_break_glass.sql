-- Every break-glass rollback. The unique nonce makes each signed request usable once.
CREATE TABLE break_glass_events (
    id               uuid PRIMARY KEY,
    key_id           text NOT NULL,
    nonce            text NOT NULL UNIQUE,
    reason           text NOT NULL,
    requested_at     timestamptz NOT NULL,
    replaced_version bigint,
    restored_version bigint NOT NULL,
    new_bundle_id    uuid NOT NULL REFERENCES policy_bundles (id),
    created_at       timestamptz NOT NULL DEFAULT now()
);
