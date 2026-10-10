-- Retiring an install (its credentials stop working, its history stays): a retired install's
-- client and environment can be registered again.
ALTER TABLE installs
    ADD COLUMN retired_at TIMESTAMPTZ,
    ADD COLUMN retired_by TEXT;

ALTER TABLE installs DROP CONSTRAINT installs_client_environment_key;

CREATE UNIQUE INDEX installs_client_environment_active ON installs (client, environment) WHERE retired_at IS NULL;
