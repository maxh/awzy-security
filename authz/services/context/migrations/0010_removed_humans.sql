-- People the IdP deleted, or renamed away from, by the userName they had. They stay in the
-- principals snapshot as inactive, so a PDP never falls back to the groups in a token issued
-- before the change. Provisioning the name again removes it from here.
CREATE TABLE removed_humans (
    user_name TEXT PRIMARY KEY,
    removed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
