-- Record whether each decision was enforced or only monitored (ADR 0007). In monitor, a deny
-- is a "would deny". Earlier rows predate enforcement points, so they count as monitor. The
-- Kafka queue table and its view are recreated with the new column.
DROP VIEW IF EXISTS decisions_mv;

DROP TABLE IF EXISTS decisions_queue;

ALTER TABLE decisions ADD COLUMN mode LowCardinality(String) DEFAULT 'monitor' AFTER verdict;

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
    mode                  String,
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
    mode,
    challenge,
    policies,
    on_behalf_of_verdict,
    on_behalf_of_policies,
    policy_version,
    context
FROM decisions_queue;
