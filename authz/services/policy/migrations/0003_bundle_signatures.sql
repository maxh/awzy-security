-- Bundles are signed when published (ECDSA P-256 over the payload in awzy-signing). PDPs
-- verify before loading, so a bundle changed here after publishing no longer loads.
ALTER TABLE policy_bundles
    ADD COLUMN signature bytea,
    ADD COLUMN signing_key_id text;
