-- PDPs wait for changes instead of polling for them (ADR 0077).

-- The principals snapshot moved: a PDP waiting on /v1/principals comes back for it.
CREATE FUNCTION notify_principals() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('awzy_principals', '');
    RETURN NULL;
END
$$;
CREATE TRIGGER snapshot_version_notify AFTER UPDATE ON snapshot_version
    FOR EACH STATEMENT EXECUTE FUNCTION notify_principals();

-- Every resource whose entity may have changed, in order: written, deleted, or classified, or
-- linked to work whose assignees changed (ADR 0046). A PDP reads it from /v1/resources/changes
-- to drop what it cached. Kept an hour; a PDP further behind drops its whole cache.
CREATE TABLE resource_changes (
    seq         bigserial PRIMARY KEY,
    resource_id text NOT NULL,
    changed_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX resource_changes_by_time ON resource_changes (changed_at);

CREATE FUNCTION record_resource_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        INSERT INTO resource_changes (resource_id) VALUES (OLD.id);
    ELSE
        INSERT INTO resource_changes (resource_id) VALUES (NEW.id);
    END IF;
    PERFORM pg_notify('awzy_resources', '');
    RETURN NULL;
END
$$;
CREATE TRIGGER resources_record_change AFTER INSERT OR UPDATE OR DELETE ON resources
    FOR EACH ROW EXECUTE FUNCTION record_resource_change();

-- A work item's resources: before and after, since either side's assignees changed.
CREATE FUNCTION record_work_item_change() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    INSERT INTO resource_changes (resource_id)
    SELECT DISTINCT r FROM unnest(
        CASE WHEN TG_OP = 'INSERT' THEN '{}'::text[] ELSE OLD.resources END ||
        CASE WHEN TG_OP = 'DELETE' THEN '{}'::text[] ELSE NEW.resources END) AS r;
    PERFORM pg_notify('awzy_resources', '');
    RETURN NULL;
END
$$;
CREATE TRIGGER work_items_record_change AFTER INSERT OR UPDATE OR DELETE ON work_items
    FOR EACH ROW EXECUTE FUNCTION record_work_item_change();
