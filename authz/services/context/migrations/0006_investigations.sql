-- Investigations awzy-reasoner ran (ADR 0071): a model's finding about one principal, after the
-- checks in code decided what it may do (ADR 0067). Written once by the reasoner; a person
-- closing one comes later.
CREATE TABLE investigations (
    id UUID PRIMARY KEY,
    -- Human | ServiceAccount
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    -- Why it was opened: "risk 63; a rejected challenge".
    trigger TEXT NOT NULL,
    -- The deterministic score when it ran: the floor for its risk.
    floor INTEGER NOT NULL CHECK (floor BETWEEN 0 AND 100),
    -- needs_review: a person decides (a proposed containment, or benign at elevated risk). done.
    status TEXT NOT NULL CHECK (status IN ('needs_review', 'done')),
    -- The checked outcome.
    verdict TEXT NOT NULL CHECK (verdict IN ('malicious', 'suspicious', 'benign', 'inconclusive')),
    risk INTEGER NOT NULL CHECK (risk BETWEEN 0 AND 100),
    summary TEXT NOT NULL,
    -- [{"id": ..., "why": ...}]: the evidence the outcome cites.
    evidence JSONB NOT NULL DEFAULT '[]',
    containment TEXT NOT NULL CHECK (containment IN ('none', 'propose', 'recommend')),
    -- What the checks changed.
    notes JSONB NOT NULL DEFAULT '[]',
    -- The model's own finding before the checks, or null when it gave none.
    finding JSONB,
    model TEXT NOT NULL,
    input_tokens BIGINT NOT NULL DEFAULT 0 CHECK (input_tokens >= 0),
    output_tokens BIGINT NOT NULL DEFAULT 0 CHECK (output_tokens >= 0),
    started_at TIMESTAMPTZ NOT NULL,
    finished_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX investigations_by_subject ON investigations (subject_type, subject_id, finished_at DESC);
CREATE INDEX investigations_by_finished ON investigations (finished_at DESC);
