-- Query plans (ADR 0113): which rows of a type a principal was shown, as the filter the PDP
-- returned. Their own table, not decisions: a plan decides no one resource. Ordered like
-- `decisions`, for "what did this principal list recently".
CREATE TABLE plans
(
    id                 UUID,
    at                 DateTime64(3, 'UTC'),
    principal_type     LowCardinality(String),
    principal_id       String,
    on_behalf_of_type  LowCardinality(String),
    on_behalf_of_id    String,
    action             LowCardinality(String),
    resource_type      LowCardinality(String),
    outcome            LowCardinality(String),
    mode               LowCardinality(String),
    challenge          LowCardinality(String),
    filter             String,
    filter_hash        String,
    policies           Array(String),
    policy_version     Int64,
    principals_version Int64,
    context            String
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(at)
ORDER BY (principal_type, principal_id, at);

-- Consumes the plans topic (awzy_model::PlanRecord as JSON; `id` is a QID, plan_<uuid>), in its
-- own consumer group.
CREATE TABLE plans_queue
(
    id                 String,
    at_ms              Int64,
    principal_type     String,
    principal_id       String,
    on_behalf_of_type  String,
    on_behalf_of_id    String,
    action             String,
    resource_type      String,
    outcome            String,
    mode               String,
    challenge          String,
    filter             String,
    filter_hash        String,
    policies           Array(String),
    policy_version     Int64,
    principals_version Int64,
    context            String
)
ENGINE = Kafka
SETTINGS
    kafka_broker_list = '{{kafka_brokers}}',
    kafka_topic_list = '{{plans_topic}}',
    kafka_group_name = '{{consumer_group}}-plans',
    kafka_format = 'JSONEachRow',
    kafka_flush_interval_ms = 1000;

CREATE MATERIALIZED VIEW plans_mv TO plans AS
SELECT
    toUUID(replaceOne(id, 'plan_', '')) AS id,
    fromUnixTimestamp64Milli(at_ms, 'UTC') AS at,
    principal_type,
    principal_id,
    on_behalf_of_type,
    on_behalf_of_id,
    action,
    resource_type,
    outcome,
    mode,
    challenge,
    filter,
    filter_hash,
    policies,
    policy_version,
    principals_version,
    context
FROM plans_queue;
