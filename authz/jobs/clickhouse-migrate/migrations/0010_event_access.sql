-- How much each event touched (#288): its access kind (listed, read, written, exported), how many
-- rows a list, search, or export returned, and the tool and action class an agent called. Earlier
-- rows stay empty. The Kafka queue table and its view are recreated with the new columns.
DROP VIEW IF EXISTS events_mv;

DROP TABLE IF EXISTS events_queue -- awzy: contract (recreated below with the new columns; #288)
;

ALTER TABLE events ADD COLUMN IF NOT EXISTS access LowCardinality(String) DEFAULT '' AFTER attributes;

ALTER TABLE events ADD COLUMN IF NOT EXISTS count UInt64 DEFAULT 0 AFTER access;

ALTER TABLE events ADD COLUMN IF NOT EXISTS tool LowCardinality(String) DEFAULT '' AFTER count;

ALTER TABLE events ADD COLUMN IF NOT EXISTS action_class LowCardinality(String) DEFAULT '' AFTER tool;

CREATE TABLE events_queue
(
    id                String,
    at_ms             Int64,
    received_at_ms    Int64,
    source            String,
    system            String,
    principal_type    String,
    principal_id      String,
    on_behalf_of_type String,
    on_behalf_of_id   String,
    action            String,
    resource_type     String,
    resource_id       String,
    outcome           String,
    source_ip         String,
    attributes        String,
    access            String,
    count             UInt64,
    tool              String,
    action_class      String
)
ENGINE = Kafka
SETTINGS
    kafka_broker_list = '{{kafka_brokers}}',
    kafka_topic_list = '{{events_topic}}',
    kafka_group_name = '{{consumer_group}}-events',
    kafka_format = 'JSONEachRow',
    kafka_flush_interval_ms = 1000;

CREATE MATERIALIZED VIEW events_mv TO events AS
SELECT
    toUUID(replaceOne(id, 'event_', '')) AS id,
    fromUnixTimestamp64Milli(at_ms, 'UTC') AS at,
    fromUnixTimestamp64Milli(received_at_ms, 'UTC') AS received_at,
    source,
    system,
    principal_type,
    principal_id,
    on_behalf_of_type,
    on_behalf_of_id,
    action,
    resource_type,
    resource_id,
    outcome,
    source_ip,
    attributes,
    access,
    count,
    tool,
    action_class
FROM events_queue;
