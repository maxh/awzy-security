//! ClickHouse schema for awzy-authz: numbered SQL migrations, applied in order and recorded
//! in `schema_migrations`.
//!
//! Migrations may use `{{placeholders}}` for settings that differ per install (see
//! [`Settings`]), such as the Kafka brokers as ClickHouse itself reaches them.

use axum::Router;
use clickhouse::Client;

/// (version, name, SQL). Append only; never edit a migration that has shipped.
const MIGRATIONS: &[(u32, &str, &str)] = &[
    (1, "decisions", include_str!("../migrations/0001_decisions.sql")),
    (2, "decisions_id_index", include_str!("../migrations/0002_decisions_id_index.sql")),
    (3, "delegation", include_str!("../migrations/0003_delegation.sql")),
    (4, "enforcement_mode", include_str!("../migrations/0004_enforcement_mode.sql")),
    (5, "events", include_str!("../migrations/0005_events.sql")),
    (6, "human_policy_ids", include_str!("../migrations/0006_human_policy_ids.sql")),
    (7, "principal", include_str!("../migrations/0007_principal.sql")),
];

/// Values substituted into migrations.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Kafka brokers as ClickHouse reaches them, e.g. `kafka:29092` in the local stack.
    pub kafka_brokers: String,
    pub decisions_topic: String,
    /// awzy-intake's events topic.
    pub events_topic: String,
    /// Each queue table consumes in a group named from this (events add `-events`).
    pub consumer_group: String,
}

impl Settings {
    fn render(&self, sql: &str) -> String {
        sql.replace("{{kafka_brokers}}", &self.kafka_brokers)
            .replace("{{decisions_topic}}", &self.decisions_topic)
            .replace("{{events_topic}}", &self.events_topic)
            .replace("{{consumer_group}}", &self.consumer_group)
    }
}

/// Applies every migration not yet recorded. Returns the versions applied.
pub async fn migrate(client: &Client, settings: &Settings) -> anyhow::Result<Vec<u32>> {
    client
        .query(
            "CREATE TABLE IF NOT EXISTS schema_migrations
             (version UInt32, name String, applied_at DateTime DEFAULT now())
             ENGINE = MergeTree ORDER BY version",
        )
        .execute()
        .await?;
    let applied: Vec<u32> = client.query("SELECT version FROM schema_migrations").fetch_all().await?;

    let mut done = Vec::new();
    for &(version, name, sql) in MIGRATIONS.iter().filter(|(v, ..)| !applied.contains(v)) {
        for statement in statements(&settings.render(sql)) {
            client.query(&statement).execute().await?;
        }
        client
            .query("INSERT INTO schema_migrations (version, name) VALUES (?, ?)")
            .bind(version)
            .bind(name)
            .execute()
            .await?;
        tracing::info!(version, name, "applied ClickHouse migration");
        done.push(version);
    }
    Ok(done)
}

/// The ClickHouse user awzy-reporter reads disk use and Kafka consumer positions with (ADR 0042).
pub const REPORTER_USER: &str = "awzy_reporter";

/// Creates `user` if needed and sets its password and grants, so each sync leaves it exactly as
/// ADR 0042 describes: it can read `system.disks` (sizes only) and `system.kafka_consumers`, and
/// see table names (`SHOW`), and nothing else. ClickHouse lists a Kafka table's consumers only
/// to users who can see the table.
pub async fn ensure_reporter_user(client: &Client, user: &str, password: &str) -> anyhow::Result<()> {
    anyhow::ensure!(user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'), "invalid user name {user:?}");
    anyhow::ensure!(password.len() >= 16, "the reporter's ClickHouse password must be at least 16 characters");
    for statement in [
        format!("CREATE USER IF NOT EXISTS {user} IDENTIFIED WITH sha256_password BY ? SETTINGS readonly = 1"),
        format!("ALTER USER {user} IDENTIFIED WITH sha256_password BY ? SETTINGS readonly = 1"),
        format!("REVOKE ALL ON *.* FROM {user}"),
        format!("GRANT SELECT(name, free_space, total_space) ON system.disks TO {user}"),
        format!("GRANT SELECT ON system.kafka_consumers TO {user}"),
        format!("GRANT SHOW ON *.* TO {user}"),
    ] {
        let query = client.query(&statement);
        let query = if statement.contains('?') { query.bind(password) } else { query };
        query.execute().await?;
    }
    tracing::info!(user, "ClickHouse reporter user is current");
    Ok(())
}

pub const REASONER_USER: &str = "awzy_reasoner";

/// Creates `user` if needed and sets its password and grants: awzy-reasoner reads `decisions`
/// and `events` in `database` to score risk (ADR 0056), and nothing else.
pub async fn ensure_reasoner_user(client: &Client, database: &str, user: &str, password: &str) -> anyhow::Result<()> {
    for name in [database, user] {
        anyhow::ensure!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'), "invalid name {name:?}");
    }
    anyhow::ensure!(password.len() >= 16, "the reasoner's ClickHouse password must be at least 16 characters");
    for statement in [
        format!("CREATE USER IF NOT EXISTS {user} IDENTIFIED WITH sha256_password BY ? SETTINGS readonly = 1"),
        format!("ALTER USER {user} IDENTIFIED WITH sha256_password BY ? SETTINGS readonly = 1"),
        format!("REVOKE ALL ON *.* FROM {user}"),
        format!("GRANT SELECT ON {database}.decisions TO {user}"),
        format!("GRANT SELECT ON {database}.events TO {user}"),
    ] {
        let query = client.query(&statement);
        let query = if statement.contains('?') { query.bind(password) } else { query };
        query.execute().await?;
    }
    tracing::info!(user, "ClickHouse reasoner user is current");
    Ok(())
}

/// ClickHouse runs one statement per request. Statements end with `;` at the end of a line.
fn statements(sql: &str) -> Vec<String> {
    sql.split(";\n")
        .map(|s| s.trim().trim_end_matches(';').trim())
        .filter(|s| s.lines().any(|l| !l.trim().is_empty() && !l.trim_start().starts_with("--")))
        .map(str::to_string)
        .collect()
}

/// The migration runner is a one-shot Job; it serves nothing.
pub fn router() -> Router {
    Router::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_statements_and_skips_comments() {
        let sql = "-- a\nCREATE TABLE a (x UInt8) ENGINE = Memory;\n\n-- b only\n;\nSELECT 1;\n";
        assert_eq!(statements(sql), vec!["-- a\nCREATE TABLE a (x UInt8) ENGINE = Memory", "SELECT 1"]);
    }

    #[test]
    fn renders_placeholders() {
        let s = Settings {
            kafka_brokers: "k:1".into(),
            decisions_topic: "t".into(),
            events_topic: "e".into(),
            consumer_group: "g".into(),
        };
        for (_, name, sql) in MIGRATIONS {
            assert!(!s.render(sql).contains("{{"), "{name}");
        }
    }

    /// With --config=local-stack: the reporter's user reads disk sizes and consumer positions,
    /// and is refused query text and awzy's tables.
    #[tokio::test]
    async fn the_reporter_user_reads_only_what_it_reports() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let admin = Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        let user = format!("test_reporter_{}", uuid::Uuid::now_v7().simple());
        let password = "test-reporter-password-0123";
        ensure_reporter_user(&admin, &user, password).await.unwrap();
        ensure_reporter_user(&admin, &user, password).await.unwrap(); // every sync
        let reporter = Client::default().with_url(&url).with_user(&user).with_password(password);

        let disks: u64 =
            reporter.query("SELECT count() FROM system.disks WHERE total_space > 0").fetch_one().await.unwrap();
        assert!(disks >= 1);
        reporter.query("SELECT count() FROM system.kafka_consumers").fetch_one::<u64>().await.unwrap();
        for denied in [
            "SELECT count() FROM system.query_log",
            "SELECT count() FROM system.processes",
            "SELECT path FROM system.disks",
            "SELECT count() FROM system.users",
        ] {
            assert!(reporter.query(denied).fetch_one::<u64>().await.is_err(), "{denied}");
        }
        admin.query(&format!("DROP USER {user}")).execute().await.unwrap();
    }

    /// With --config=local-stack: the reasoner's user reads decisions and events, and nothing
    /// else, and can't write.
    #[tokio::test]
    async fn the_reasoner_user_reads_only_activity() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let suffix = uuid::Uuid::now_v7().simple().to_string();
        let database = format!("test_{suffix}");
        let admin = Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        admin.query(&format!("CREATE DATABASE {database}")).execute().await.unwrap();
        let settings = Settings {
            kafka_brokers: "localhost:1".into(),
            decisions_topic: format!("unused.{database}"),
            events_topic: format!("unused-events.{database}"),
            consumer_group: database.clone(),
        };
        migrate(&admin.clone().with_database(&database), &settings).await.unwrap();
        let user = format!("test_reasoner_{suffix}");
        let password = "test-reasoner-password-0123";
        ensure_reasoner_user(&admin, &database, &user, password).await.unwrap();
        ensure_reasoner_user(&admin, &database, &user, password).await.unwrap(); // every sync
        let reasoner =
            Client::default().with_url(&url).with_user(&user).with_password(password).with_database(&database);

        for allowed in ["SELECT count() FROM decisions", "SELECT count() FROM events"] {
            reasoner.query(allowed).fetch_one::<u64>().await.unwrap();
        }
        for denied in ["SELECT count() FROM decisions_queue", "SELECT count() FROM system.query_log"] {
            assert!(reasoner.query(denied).fetch_one::<u64>().await.is_err(), "{denied}");
        }
        assert!(reasoner.query("TRUNCATE TABLE events").execute().await.is_err());
        admin.query(&format!("DROP USER {user}")).execute().await.unwrap();
        admin.query(&format!("DROP DATABASE {database}")).execute().await.unwrap();
    }

    /// End to end with --config=local-stack: the PDP's decision log and awzy-intake's events ->
    /// Kafka -> ClickHouse.
    #[tokio::test]
    async fn decisions_and_events_flow_from_kafka_into_clickhouse() {
        let (Ok(url), Ok(host_brokers), Ok(clickhouse_brokers)) = (
            std::env::var("AWZY_TEST_CLICKHOUSE_URL"),
            std::env::var("AWZY_TEST_KAFKA_BROKERS"),
            std::env::var("AWZY_TEST_CLICKHOUSE_KAFKA_BROKERS"),
        ) else {
            eprintln!("skipping: local-stack environment is not set");
            return;
        };
        let suffix = uuid::Uuid::now_v7().simple().to_string();
        let database = format!("test_{suffix}");
        let topic = format!("test.decisions.{suffix}");
        let events_topic = format!("test.events.{suffix}");
        let admin = Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        admin.query(&format!("CREATE DATABASE {database}")).execute().await.unwrap();
        let client = admin.clone().with_database(&database);

        // The topics must exist before ClickHouse subscribes to them; connecting creates them.
        let brokers: Vec<String> = host_brokers.split(',').map(str::to_string).collect();
        let log = awzy_pdp::DecisionLog::connect_kafka(brokers.clone(), &topic).await.unwrap();
        let events = awzy_intake::EventLog::connect_kafka(brokers, &events_topic).await.unwrap();
        let settings = Settings {
            kafka_brokers: clickhouse_brokers,
            decisions_topic: topic,
            events_topic,
            consumer_group: database.clone(),
        };
        let every: Vec<u32> = MIGRATIONS.iter().map(|(v, ..)| *v).collect();
        assert_eq!(migrate(&client, &settings).await.unwrap(), every);
        assert!(migrate(&client, &settings).await.unwrap().is_empty(), "migrations apply once");

        let pdp = awzy_pdp::Pdp::with_defaults().unwrap();
        let req = awzy_model::Check {
            principal: awzy_model::EntityRef::new("ServiceAccount", "ops-bot"),
            action: "read".into(),
            resource: awzy_model::EntityRef::new("Resource", "board-deck"),
            context: serde_json::from_str(r#"{"risk_score": 10}"#).unwrap(),
            on_behalf_of: Some(awzy_model::EntityRef::new("Human", "alice")),
            human_present: Some(true),
            always_enforced: false,
        };
        let entities = pdp
            .entities(serde_json::from_str(
                r#"[{"uid":{"type":"Human","id":"alice"},"attrs":{},"parents":[{"type":"Group","id":"security"}]},
                    {"uid":{"type":"ServiceAccount","id":"ops-bot"},"attrs":{"owner":{"__entity":{"type":"Human","id":"alice"}},"ai":true},"parents":[{"type":"Group","id":"security"}]},
                    {"uid":{"type":"Resource","id":"board-deck"},"attrs":{"sensitivity":"crown_jewels"},"parents":[]}]"#,
            )
            .unwrap())
            .unwrap();
        let mut decision = pdp.decide(&req, &entities).unwrap();
        decision.mode = awzy_model::EnforcementMode::Enforce;
        log.record(awzy_model::DecisionRecord::new(&req, &decision, 0, chrono::Utc::now()));

        let now = chrono::Utc::now();
        let event = awzy_model::EventRecord {
            id: awzy_model::EventId::new(),
            at: now,
            received_at: now,
            source: "aws_cloudtrail".into(),
            system: "s3.amazonaws.com".into(),
            principal_type: "AwsPrincipal".into(),
            principal_id: "arn:aws:sts::111122223333:assumed-role/Analyst/carol".into(),
            on_behalf_of_type: String::new(),
            on_behalf_of_id: String::new(),
            action: "GetObject".into(),
            resource_type: "AWS::S3::Object".into(),
            resource_id: "arn:aws:s3:::larkspur-finance/q3-plan.xlsx".into(),
            outcome: "failure".into(),
            source_ip: "203.0.113.7".into(),
            attributes: r#"{"errorCode":"AccessDenied"}"#.into(),
        };
        events.send(std::slice::from_ref(&event)).await.unwrap();

        let mut found = 0u64;
        for _ in 0..60 {
            found = client
                .query(
                    "SELECT count() FROM decisions WHERE id = toUUID(?) AND verdict = 'allow' AND principal_id = 'ops-bot'
                     AND on_behalf_of_id = 'alice' AND human_present AND on_behalf_of_verdict = 'allow'
                     AND has(on_behalf_of_policies, 'security-team-read') AND mode = 'enforce'",
                )
                .bind(decision.id.uuid().to_string())
                .fetch_one()
                .await
                .unwrap();
            if found == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        let mut found_event = 0u64;
        for _ in 0..60 {
            found_event = client
                .query(
                    "SELECT count() FROM events WHERE id = toUUID(?) AND source = 'aws_cloudtrail' AND action = 'GetObject'
                     AND principal_id = 'arn:aws:sts::111122223333:assumed-role/Analyst/carol' AND outcome = 'failure'
                     AND at = fromUnixTimestamp64Milli(?, 'UTC')",
                )
                .bind(event.id.uuid().to_string())
                .bind(event.at.timestamp_millis())
                .fetch_one()
                .await
                .unwrap();
            if found_event == 1 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        admin.query(&format!("DROP DATABASE {database}")).execute().await.unwrap();
        assert_eq!(found, 1, "decision did not reach ClickHouse within 30s");
        assert_eq!(found_event, 1, "event did not reach ClickHouse within 30s");
    }
}
