-- Who touched a resource: decisions and events read in resource order, beside the principal
-- order the tables keep (platform plan, "ClickHouse in detail").
ALTER TABLE decisions ADD PROJECTION IF NOT EXISTS by_resource (SELECT * ORDER BY resource_id, at);

ALTER TABLE decisions MATERIALIZE PROJECTION by_resource;

ALTER TABLE events ADD PROJECTION IF NOT EXISTS by_resource (SELECT * ORDER BY resource_id, at);

ALTER TABLE events MATERIALIZE PROJECTION by_resource;
