-- The work item each request named (ADR 0118): a ticket or task in the client's own systems,
-- which an agent's token says it's doing. Empty when the request named none, and in earlier rows.
-- The Kafka queue table and its view are recreated with the new column.
DROP VIEW IF EXISTS decisions_mv;

DROP TABLE IF EXISTS decisions_queue -- awzy: contract (recreated below with the new column; ADR 0118)
;

ALTER TABLE decisions ADD COLUMN IF NOT EXISTS work_item_id String DEFAULT '' AFTER resource_attributes_from;

CREATE TABLE decisions_queue
(
    id                       String,
    at_ms                    Int64,
    principal_type           String,
    principal_id             String,
    on_behalf_of_type        String,
    on_behalf_of_id          String,
    human_present            Bool,
    action                   String,
    resource_type            String,
    resource_id              String,
    verdict                  String,
    mode                     String,
    challenge                String,
    policies                 Array(String),
    on_behalf_of_verdict     String,
    on_behalf_of_policies    Array(String),
    policy_version           Int64,
    context                  String,
    resource_attributes_from String,
    work_item_id             String
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
    principal_type,
    principal_id,
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
    context,
    resource_attributes_from,
    work_item_id
FROM decisions_queue;
