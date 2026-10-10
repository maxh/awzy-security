-- A human's ID is `human_id` (ADR 0075). The expand step of a rename (ADR 0072): `human_id` is
-- added beside `person_id` and filled in, and a trigger keeps the two equal on every write, so an
-- older awzy-challenge still writing `person_id` and this one writing `human_id` both work during
-- a rollout. A later release drops `person_id`, the trigger, and its index.
ALTER TABLE security_keys ADD COLUMN human_id text;
ALTER TABLE webauthn_ceremonies ADD COLUMN human_id text;
UPDATE security_keys SET human_id = person_id;
UPDATE webauthn_ceremonies SET human_id = person_id;

CREATE FUNCTION same_human_and_person_id() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    NEW.human_id := COALESCE(NEW.human_id, NEW.person_id);
    NEW.person_id := COALESCE(NEW.person_id, NEW.human_id);
    RETURN NEW;
END
$$;

CREATE TRIGGER security_keys_same_human_and_person_id
    BEFORE INSERT OR UPDATE ON security_keys
    FOR EACH ROW EXECUTE FUNCTION same_human_and_person_id();
CREATE TRIGGER webauthn_ceremonies_same_human_and_person_id
    BEFORE INSERT OR UPDATE ON webauthn_ceremonies
    FOR EACH ROW EXECUTE FUNCTION same_human_and_person_id();

CREATE INDEX security_keys_active_by_human ON security_keys (human_id) WHERE removed_at IS NULL;
