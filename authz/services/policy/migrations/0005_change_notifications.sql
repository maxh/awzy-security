-- PDPs wait for a new bundle or enforcement mode instead of polling for one (ADR 0077).
CREATE FUNCTION notify_policy() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('awzy_policy', '');
    RETURN NULL;
END
$$;
CREATE TRIGGER policy_bundles_notify AFTER INSERT OR UPDATE ON policy_bundles
    FOR EACH STATEMENT EXECUTE FUNCTION notify_policy();
CREATE TRIGGER enforcement_mode_changes_notify AFTER INSERT ON enforcement_mode_changes
    FOR EACH STATEMENT EXECUTE FUNCTION notify_policy();
