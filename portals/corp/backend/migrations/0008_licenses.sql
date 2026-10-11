-- What each client is licensed for (#142): a plan, how many people it covers, when it ends, and
-- the features it includes beyond the base product. One row per client, keyed by the slug its
-- installs carry, so every install of a client shares it. Staff set it in the corp portal.
CREATE TABLE licenses (
    client       TEXT PRIMARY KEY,
    plan         TEXT NOT NULL CHECK (plan IN ('trial', 'standard', 'enterprise')),
    humans       INTEGER NOT NULL CHECK (humans > 0),
    expires_at   TIMESTAMPTZ,
    entitlements TEXT[] NOT NULL DEFAULT '{}',
    updated_by   TEXT NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
