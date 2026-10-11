-- Support sessions (ADR 0095): awzy staff ask through the corp portal, a client admin decides here,
-- and the staff member may view the approved portal areas until the session expires or an admin
-- ends it. The corp portal mints the ID. Every operation in a session is a support_actions row.
CREATE TABLE support_sessions (
    id               UUID PRIMARY KEY,
    requested_by     TEXT NOT NULL,
    areas            TEXT[] NOT NULL,
    reason           TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL CHECK (duration_minutes > 0),
    requested_at     TIMESTAMPTZ NOT NULL,
    received_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    status           TEXT NOT NULL DEFAULT 'requested' CHECK (status IN ('requested', 'approved', 'rejected', 'ended')),
    approved_areas   TEXT[] NOT NULL DEFAULT '{}',
    expires_at       TIMESTAMPTZ,
    decided_by       TEXT,
    decided_at       TIMESTAMPTZ,
    ended_by         TEXT,
    ended_at         TIMESTAMPTZ
);

CREATE TABLE support_actions (
    id         UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES support_sessions (id),
    operation  TEXT NOT NULL,
    at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX support_actions_by_session ON support_actions (session_id, at);
