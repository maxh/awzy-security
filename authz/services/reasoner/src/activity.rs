//! Each principal's activity per day, from ClickHouse: the decisions the PDPs made and the
//! events awzy-intake received, together. A "day" is 24 hours counted back from the run, so
//! day 0 is always the last full 24 hours whatever time the reasoner runs.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::score::Day;

/// Days of history each run reads: the last one, scored, and the ones before, compared with.
pub const HISTORY_DAYS: i64 = 30;

/// A person or service account, as `(type, id)`.
pub type Principal = (String, String);

/// Each principal's active days, by how many days before the run they were (0 is the last).
pub type History = BTreeMap<Principal, BTreeMap<u32, Day>>;

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct Row {
    subject_type: String,
    subject_id: String,
    days_ago: u32,
    accesses: u64,
    resources: u64,
    denied: u64,
}

// Only denials (and events a system refused) count against a principal, not challenges:
// high-risk-verify challenges whoever this scores highly, and counting those would feed the
// score back into itself.
const QUERY: &str = "
SELECT
    subject_type,
    subject_id,
    toUInt32(intDiv(? - toUnixTimestamp64Milli(at), 86400000)) AS days_ago,
    count() AS accesses,
    uniqExact(resource_id) AS resources,
    sum(denied) AS denied
FROM
(
    SELECT at, subject_type, subject_id, resource_id, toUInt8(verdict = 'deny') AS denied
    FROM decisions
    WHERE at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
    UNION ALL
    SELECT at, subject_type, subject_id, resource_id, toUInt8(outcome = 'failure') AS denied
    FROM events
    WHERE at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
)
WHERE subject_type IN ('Human', 'ServiceAccount')
GROUP BY subject_type, subject_id, days_ago";

/// The [`HISTORY_DAYS`] before `as_of`.
pub async fn load(clickhouse: &clickhouse::Client, as_of: DateTime<Utc>) -> anyhow::Result<History> {
    let end = as_of.timestamp_millis();
    let start = (as_of - chrono::Duration::days(HISTORY_DAYS)).timestamp_millis();
    let rows: Vec<Row> =
        clickhouse.query(QUERY).bind(end).bind(start).bind(end).bind(start).bind(end).fetch_all().await?;
    let mut history = History::new();
    for r in rows {
        let day = Day { accesses: r.accesses, resources: r.resources, denied: r.denied };
        history.entry((r.subject_type, r.subject_id)).or_default().insert(r.days_ago, day);
    }
    Ok(history)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With --config=local-stack: a month of an analyst team's reads, then one analyst opening
    /// six times as much as usual on the last day.
    #[tokio::test]
    async fn reads_each_principals_days_back_from_the_run() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let database = format!("test_{}", uuid::Uuid::now_v7().simple());
        let admin = clickhouse::Client::default().with_url(&url).with_user("awzy").with_password("awzy");
        admin.query(&format!("CREATE DATABASE {database}")).execute().await.unwrap();
        let ch = admin.clone().with_database(&database);
        let settings = awzy_clickhouse::Settings {
            kafka_brokers: "localhost:1".into(),
            decisions_topic: format!("unused.{database}"),
            events_topic: format!("unused-events.{database}"),
            consumer_group: database.clone(),
        };
        awzy_clickhouse::migrate(&ch, &settings).await.unwrap();

        let as_of = Utc::now();
        let mut events = Vec::new();
        for days_ago in 0..20i64 {
            for analyst in ["ana", "ben", "cy", "dee"] {
                let opened = if analyst == "ana" && days_ago == 0 { 60 } else { 10 };
                for n in 0..opened {
                    let at = (as_of - chrono::Duration::days(days_ago) - chrono::Duration::minutes(n + 1))
                        .timestamp_millis();
                    events.push(format!(
                        "(generateUUIDv4(), fromUnixTimestamp64Milli({at}, 'UTC'), 'Human', '{analyst}', 'Resource', 'doc-{days_ago}-{n}')"
                    ));
                }
            }
        }
        ch.query(&format!(
            "INSERT INTO events (id, at, subject_type, subject_id, resource_type, resource_id) VALUES {}",
            events.join(",")
        ))
        .execute()
        .await
        .unwrap();
        // One denied decision for ben today, and a group, which isn't a principal.
        let now = as_of.timestamp_millis() - 1000;
        ch.query(&format!(
            "INSERT INTO decisions (id, at, subject_type, subject_id, resource_type, resource_id, verdict) VALUES
             (generateUUIDv4(), fromUnixTimestamp64Milli({now}, 'UTC'), 'Human', 'ben', 'Resource', 'board-deck', 'deny'),
             (generateUUIDv4(), fromUnixTimestamp64Milli({now}, 'UTC'), 'Group', 'analysts', 'Resource', 'x', 'allow')"
        ))
        .execute()
        .await
        .unwrap();

        let history = load(&ch, as_of).await.unwrap();
        admin.query(&format!("DROP DATABASE {database}")).execute().await.unwrap();

        assert_eq!(history.len(), 4, "groups aren't principals");
        let ana = &history[&("Human".to_string(), "ana".to_string())];
        assert_eq!(ana.len(), 20);
        assert_eq!(ana[&0], Day { accesses: 60, resources: 60, denied: 0 });
        assert_eq!(ana[&19].resources, 10);
        let ben = &history[&("Human".to_string(), "ben".to_string())];
        assert_eq!(ben[&0], Day { accesses: 11, resources: 11, denied: 1 });
    }
}
