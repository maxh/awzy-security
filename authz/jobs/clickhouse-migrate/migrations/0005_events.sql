-- Events from awzy-intake: what accessors did as app and audit logs recorded it, next to what
-- the PDPs decided. Ordered like `decisions`, for "what did this accessor do recently".
CREATE TABLE events
(
    id                UUID,
    at                DateTime64(3, 'UTC'),
    received_at       DateTime64(3, 'UTC'),
    source            LowCardinality(String),
    system            LowCardinality(String),
    subject_type      LowCardinality(String),
    subject_id        String,
    on_behalf_of_type LowCardinality(String),
    on_behalf_of_id   String,
    action            LowCardinality(String),
    resource_type     LowCardinality(String),
    resource_id       String,
    outcome           LowCardinality(String),
    source_ip         String,
    attributes        String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(at)
ORDER BY (subject_type, subject_id, at);

-- Consumes the events topic (awzy_model::EventRecord as JSON; `id` is a QID, event_<uuid>),
-- in its own consumer group so it doesn't share offsets or rebalances with decisions.
CREATE TABLE events_queue
(
    id                String,
    at_ms             Int64,
    received_at_ms    Int64,
    source            String,
    system            String,
    subject_type      String,
    subject_id        String,
    on_behalf_of_type String,
    on_behalf_of_id   String,
    action            String,
    resource_type     String,
    resource_id       String,
    outcome           String,
    source_ip         String,
    attributes        String
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
    subject_type,
    subject_id,
    on_behalf_of_type,
    on_behalf_of_id,
    action,
    resource_type,
    resource_id,
    outcome,
    source_ip,
    attributes
FROM events_queue;
