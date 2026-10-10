-- Service accounts: backend services, scheduled jobs, and AI agents, registered by the client's
-- admins with an accountable owner (ADR 0016, ADR 0033).
CREATE TABLE service_accounts (
    -- The ServiceAccount entity ID: the token's sub, or its act.sub when acting for a person.
    id TEXT PRIMARY KEY,
    -- A person's userName (humans.user_name): who is accountable and answers its challenges.
    owner TEXT NOT NULL,
    ai BOOLEAN NOT NULL,
    -- Group IDs it belongs to, as policies name them.
    groups TEXT[] NOT NULL DEFAULT '{}',
    description TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
