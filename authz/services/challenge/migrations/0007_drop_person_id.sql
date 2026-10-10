-- Finishes the rename to `human_id` (ADR 0076). No install depended on `person_id`, so the
-- contract step follows the expand step at once rather than a release later.
DROP TRIGGER security_keys_same_human_and_person_id ON security_keys;
DROP TRIGGER webauthn_ceremonies_same_human_and_person_id ON webauthn_ceremonies;
DROP FUNCTION same_human_and_person_id();
DROP INDEX security_keys_active_by_person;
ALTER TABLE security_keys DROP COLUMN person_id; -- awzy: contract (no install used it; ADR 0076)
ALTER TABLE webauthn_ceremonies DROP COLUMN person_id; -- awzy: contract (no install used it; ADR 0076)
ALTER TABLE security_keys ALTER COLUMN human_id SET NOT NULL; -- awzy: contract (filled for every row by 0006)
ALTER TABLE webauthn_ceremonies ALTER COLUMN human_id SET NOT NULL; -- awzy: contract (filled for every row by 0006)
