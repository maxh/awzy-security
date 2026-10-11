-- Enforcement mode changes are signed like bundles (ADR 0080), so switching an install to monitor
-- in this table doesn't change what PDPs enforce.
ALTER TABLE enforcement_mode_changes
    ADD COLUMN signature bytea,
    ADD COLUMN signing_key_id text;
