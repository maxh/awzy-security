-- Support sessions awzy staff ask for (ADR 0095). The install decides each one: the request
-- reaches it in a heartbeat's answer, and its heartbeats report where the session stands. Only the
-- ID, status, and expiry come back; the decision and everything done in it stay in the install.
CREATE TABLE support_sessions (
    id               UUID PRIMARY KEY,
    install_id       UUID NOT NULL REFERENCES installs (id),
    requested_by     TEXT NOT NULL,
    areas            TEXT[] NOT NULL,
    reason           TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL CHECK (duration_minutes > 0),
    portal_url       TEXT NOT NULL,
    requested_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    status           TEXT NOT NULL DEFAULT 'requested',
    expires_at       TIMESTAMPTZ,
    reported_at      TIMESTAMPTZ
);

CREATE INDEX support_sessions_by_install ON support_sessions (install_id, requested_at DESC);
