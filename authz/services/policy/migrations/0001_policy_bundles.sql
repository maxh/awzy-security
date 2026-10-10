CREATE TABLE policy_bundles (
    id           uuid PRIMARY KEY,
    version      bigint GENERATED ALWAYS AS IDENTITY UNIQUE,
    schema_src   text NOT NULL,
    policies_src text NOT NULL,
    status       text NOT NULL CHECK (status IN ('draft', 'active', 'superseded')),
    created_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    published_by text,
    published_at timestamptz
);

-- At most one active bundle.
CREATE UNIQUE INDEX policy_bundles_one_active ON policy_bundles ((true)) WHERE status = 'active';
