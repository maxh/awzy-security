-- Who a decision or event is about is the principal (ADR 0079). ClickHouse can't rename a
-- column in a table's sort key, so both logs are rebuilt with `principal_type` and
-- `principal_id`, their rows copied across, and the Kafka queues and views recreated to match.
DROP VIEW IF EXISTS decisions_mv;

DROP TABLE IF EXISTS decisions_queue -- awzy: contract (recreated below; ADR 0079)
;

CREATE TABLE decisions_principal
(
    id                    UUID,
    at                    DateTime64(3, 'UTC'),
    principal_type        LowCardinality(String),
    principal_id          String,
    on_behalf_of_type     LowCardinality(String),
    on_behalf_of_id       String,
    human_present         Bool,
    action                LowCardinality(String),
    resource_type         LowCardinality(String),
    resource_id           String,
    verdict               LowCardinality(String),
    mode                  LowCardinality(String) DEFAULT 'monitor',
    challenge             LowCardinality(String),
    policies              Array(String),
    on_behalf_of_verdict  LowCardinality(String),
    on_behalf_of_policies Array(String),
    policy_version        Int64,
    context               String,
    INDEX id_bloom id TYPE bloom_filter GRANULARITY 4
)
ENGINE = MergeTree
PARTITION BY toYYYYMM(at)
ORDER BY (principal_type, principal_id, at);

INSERT INTO decisions_principal
SELECT id, at, subject_type, subject_id, on_behalf_of_type, on_behalf_of_id, human_present, action,
       resource_type, resource_id, verdict, mode, challenge, policies, on_behalf_of_verdict,
       on_behalf_of_policies, policy_version, context
FROM decisions;

EXCHANGE TABLES decisions AND decisions_principal;

DROP TABLE decisions_principal -- awzy: contract (the old decisions, after the exchange; ADR 0079)
;

CREATE TABLE decisions_queue
(
    id                    String,
    at_ms                 Int64,
    principal_type        String,
    principal_id          String,
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
    context
FROM decisions_queue;

DROP VIEW IF EXISTS events_mv;

DROP TABLE IF EXISTS events_queue -- awzy: contract (recreated below; ADR 0079)
;

CREATE TABLE events_principal
(
    id                UUID,
    at                DateTime64(3, 'UTC'),
    received_at       DateTime64(3, 'UTC'),
    source            LowCardinality(String),
    system            LowCardinality(String),
    principal_type    LowCardinality(String),
    principal_id      String,
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
ORDER BY (principal_type, principal_id, at);

INSERT INTO events_principal
SELECT id, at, received_at, source, system, subject_type, subject_id, on_behalf_of_type,
       on_behalf_of_id, action, resource_type, resource_id, outcome, source_ip, attributes
FROM events;

EXCHANGE TABLES events AND events_principal;

DROP TABLE events_principal -- awzy: contract (the old events, after the exchange; ADR 0079)
;

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
    principal_type,
    principal_id,
    on_behalf_of_type,
    on_behalf_of_id,
    action,
    resource_type,
    resource_id,
    outcome,
    source_ip,
    attributes
FROM events_queue;
