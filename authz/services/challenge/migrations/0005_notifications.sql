-- What awzy-challenge tells people (ADR 0073), recorded before it's sent, so a slow or failing
-- channel never delays a decision and nothing is lost across a restart. A worker sends due rows,
-- retrying with backoff; any replica can, claiming rows with FOR UPDATE SKIP LOCKED.
CREATE TABLE notifications (
    id              uuid PRIMARY KEY,
    -- challenge_opened | challenge_decided | containment
    kind            text NOT NULL,
    -- person:<userName> | security
    recipient       text NOT NULL,
    -- What the message is about, as the renderer needs it: QIDs, the request, the decision.
    details         jsonb NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    attempts        integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    sent_at         timestamptz,
    last_error      text
);

CREATE INDEX notifications_due ON notifications (next_attempt_at) WHERE sent_at IS NULL;
