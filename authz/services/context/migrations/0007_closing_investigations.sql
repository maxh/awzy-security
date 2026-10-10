-- A person closes an investigation in the client portal (ADR 0071): who, when, and why. A closed
-- investigation stays, as the record of that review.
ALTER TABLE investigations
    ADD COLUMN closed_by TEXT,
    ADD COLUMN closed_at TIMESTAMPTZ,
    ADD COLUMN close_reason TEXT;

ALTER TABLE investigations DROP CONSTRAINT investigations_status_check;
ALTER TABLE investigations
    ADD CONSTRAINT investigations_status_check CHECK (status IN ('needs_review', 'done', 'closed'));
