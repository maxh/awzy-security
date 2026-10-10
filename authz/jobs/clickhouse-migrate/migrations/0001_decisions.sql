-- Every decision the PDPs make. Ordered for "what did this accessor do recently".
CREATE TABLE decisions
(
    id              UUID,
    at              DateTime64(3, 'UTC'),
    subject_type    LowCardinality(String),
    subject_id      String,
    controller_type LowCardinality(String),
    controller_id   String,
    action          LowCardinality(String),
    resource_type   LowCardinality(String),
    resource_id     String,
    verdict         LowCardinality(String),
    challenge       LowCardinality(String),
    policies        Array(String),
    policy_version  Int64,
    context         String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(at)
ORDER BY (subject_type, subject_id, at);

-- Consumes the decisions topic (awzy_model::DecisionRecord as JSON; `id` is a QID,
-- decision_<uuid>). The view below moves each block it reads into `decisions`.
CREATE TABLE decisions_queue
(
    id              String,
    at_ms           Int64,
    subject_type    String,
    subject_id      String,
    controller_type String,
    controller_id   String,
    action          String,
    resource_type   String,
    resource_id     String,
    verdict         String,
    challenge       String,
    policies        Array(String),
    policy_version  Int64,
    context         String
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
    controller_type,
    controller_id,
    action,
    resource_type,
    resource_id,
    verdict,
    challenge,
    policies,
    policy_version,
    context
FROM decisions_queue;
