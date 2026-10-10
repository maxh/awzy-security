-- Installs awzy staff have registered (ADR 0026), with the latest heartbeat each one sent.
CREATE TABLE installs (
    id UUID PRIMARY KEY,
    client TEXT NOT NULL,
    environment TEXT NOT NULL,
    cloud TEXT NOT NULL,
    region TEXT NOT NULL,
    -- stable | fast | pinned
    channel TEXT NOT NULL,
    pinned_version TEXT,
    -- SHA-256 of the reporting token; the token itself is shown once and never stored.
    token_sha256 BYTEA NOT NULL UNIQUE,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_heartbeat JSONB,
    last_seen_at TIMESTAMPTZ,
    UNIQUE (client, environment)
);

-- When each install first reported each chart version: its upgrade history.
CREATE TABLE install_versions (
    install_id UUID NOT NULL REFERENCES installs (id),
    chart_version TEXT NOT NULL,
    first_seen_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (install_id, chart_version)
);
