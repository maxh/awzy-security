-- Risk scores awzy-reasoner computes for people and service accounts (ADR 0056), served to the
-- PDPs with the principals snapshot. One row per principal, replaced on each scoring run.
CREATE TABLE risk_scores (
    -- Human | ServiceAccount
    subject_type TEXT NOT NULL,
    -- A person's userName, or a service account's ID.
    subject_id TEXT NOT NULL,
    -- 0 (nothing unusual) to 100.
    score INTEGER NOT NULL CHECK (score BETWEEN 0 AND 100),
    -- Why, for the client portal and investigations: ["3.1x their usual daily reads", ...].
    reasons JSONB NOT NULL DEFAULT '[]',
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (subject_type, subject_id)
);
