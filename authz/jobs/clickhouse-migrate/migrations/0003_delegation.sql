-- Delegation replaces the single "controller": who a service account acted for, whether a
-- person was present, and the result of that person's own check. The Kafka queue table and
-- its view are recreated with the new columns; the consumer group keeps its offsets.
DROP VIEW IF EXISTS decisions_mv;

DROP TABLE IF EXISTS decisions_queue;

ALTER TABLE decisions
    RENAME COLUMN controller_type TO on_behalf_of_type,
    RENAME COLUMN controller_id TO on_behalf_of_id;

ALTER TABLE decisions
    ADD COLUMN human_present Bool AFTER on_behalf_of_id,
    ADD COLUMN on_behalf_of_verdict LowCardinality(String) AFTER policies,
    ADD COLUMN on_behalf_of_policies Array(String) AFTER on_behalf_of_verdict;

CREATE TABLE decisions_queue
(
    id                    String,
    at_ms                 Int64,
    subject_type          String,
    subject_id            String,
    on_behalf_of_type     String,
    on_behalf_of_id       String,
    human_present         Bool,
    action                String,
    resource_type         String,
    resource_id           String,
    verdict               String,
    challenge             String,
    policies              Array(String),
    on_behalf_of_verdict  String,
    on_behalf_of_policies Array(String),
    policy_version        Int64,
    context               String
)
ENGINE = Kafka
SETTINGS
    kafka_broker_list = '{{kafka_brokers}}',
    kafka_topic_list = '{{decisions_topic}}',
    kafka_group_name = '{{consumer_group}}',
    kafka_format = 'JSONEachRow',
    kafka_flush_interval_ms = 1000;

CREATE MATERIALIZED VIEW decisions_mv TO decisions AS
SELECT
    toUUID(replaceOne(id, 'decision_', '')) AS id,
    fromUnixTimestamp64Milli(at_ms, 'UTC') AS at,
    subject_type,
    subject_id,
    on_behalf_of_type,
    on_behalf_of_id,
    human_present,
    action,
    resource_type,
    resource_id,
    verdict,
    challenge,
    policies,
    on_behalf_of_verdict,
    on_behalf_of_policies,
    policy_version,
    context
FROM decisions_queue;
