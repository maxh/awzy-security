-- Each connector's settings, set in the client portal (ADR 0104): Drive's subject and domain,
-- Jira's site, account, and projects. Its credential isn't here; it's in the cloud's secrets
-- manager, and `credential_set_at` only says when the portal last wrote it. awzy-connectors reads
-- this every minute and restarts a connector whose settings changed.
CREATE TABLE connector_settings (
    name              TEXT PRIMARY KEY CHECK (name IN ('drive', 'jira')),
    enabled           BOOLEAN NOT NULL DEFAULT TRUE,
    settings          JSONB NOT NULL,
    credential_set_at TIMESTAMPTZ,
    updated_by        TEXT NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
