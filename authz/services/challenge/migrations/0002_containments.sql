-- Accessors whose access is revoked. Rows are kept after a lift as the audit trail.
CREATE TABLE containments (
    id           uuid PRIMARY KEY,
    subject_type text NOT NULL,
    subject_id   text NOT NULL,
    reason       text NOT NULL,
    created_by   text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    lifted_at    timestamptz,
    lifted_by    text,
    lift_reason  text
);

CREATE UNIQUE INDEX containments_one_active ON containments (subject_type, subject_id) WHERE lifted_at IS NULL;
CREATE INDEX containments_by_created ON containments (created_at DESC);

-- The feed version PDPs follow: bumped in the same transaction as every containment and lift.
CREATE TABLE containment_feed (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    version   bigint NOT NULL
);
INSERT INTO containment_feed (version) VALUES (0);
