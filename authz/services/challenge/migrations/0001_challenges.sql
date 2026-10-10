-- One row per challenge opened for a request. A request is the exact (subject, on_behalf_of,
-- action, resource) the PDP decided; empty strings stand in for "not delegated" so the request
-- columns can be unique. Kinds are ChallengeKind names ("justification", "approval", ...).
CREATE TABLE challenges (
    id                uuid PRIMARY KEY,
    decision_id       uuid NOT NULL,
    subject_type      text NOT NULL,
    subject_id        text NOT NULL,
    on_behalf_of_type text NOT NULL DEFAULT '',
    on_behalf_of_id   text NOT NULL DEFAULT '',
    action            text NOT NULL,
    resource_type     text NOT NULL,
    resource_id       text NOT NULL,
    kinds             text[] NOT NULL,
    passed            text[] NOT NULL DEFAULT '{}',
    -- A pending row past expires_at reads as expired; Resolve marks it so before opening another.
    status            text NOT NULL CHECK (status IN ('pending', 'passed', 'rejected', 'expired')),
    answerer_id       text NOT NULL DEFAULT '',
    justification     text,
    decided_by        text,
    decision_note     text,
    created_at        timestamptz NOT NULL DEFAULT now(),
    expires_at        timestamptz NOT NULL,
    passed_at         timestamptz,
    grant_expires_at  timestamptz
);

-- At most one pending challenge per request, so retries share it instead of opening more.
CREATE UNIQUE INDEX challenges_one_pending
    ON challenges (subject_type, subject_id, on_behalf_of_type, on_behalf_of_id, action, resource_type, resource_id)
    WHERE status = 'pending';
CREATE INDEX challenges_by_request
    ON challenges (subject_type, subject_id, action, resource_type, resource_id, created_at DESC);
CREATE INDEX challenges_by_answerer ON challenges (answerer_id, created_at DESC);
