//! ClickHouse disk use and Kafka lag (ADR 0042), measured with the reporter's own read-only
//! ClickHouse user and Kafka's offsets. Either is left out when its source can't be reached.
//!
//! Lag is how many records ClickHouse's Kafka consumers have yet to read: summed over every
//! partition they're assigned, the partition's latest offset minus the consumer's position.

use std::{collections::BTreeMap, time::Duration};

use rskafka::client::{
    partition::{OffsetAt, UnknownTopicHandling},
    Client as KafkaClient,
};

const TIMEOUT: Duration = Duration::from_secs(5);

/// How to reach ClickHouse and Kafka: AWZY_CLICKHOUSE_URL, AWZY_CLICKHOUSE_USER (default
/// `awzy_reporter`), AWZY_CLICKHOUSE_PASSWORD, and AWZY_KAFKA_BROKERS (with awzy-kafka's TLS
/// settings). Without ClickHouse,
/// neither figure is measured; without Kafka, only disk use is.
#[derive(Debug, Clone, Default)]
pub struct DataConfig {
    pub clickhouse: Option<ClickHouseConfig>,
    pub kafka: Option<awzy_kafka::Brokers>,
}

#[derive(Clone)]
pub struct ClickHouseConfig {
    pub url: String,
    pub user: String,
    pub password: String,
}

impl std::fmt::Debug for ClickHouseConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseConfig").field("url", &self.url).field("user", &self.user).finish_non_exhaustive()
    }
}

impl DataConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let clickhouse = match (std::env::var("AWZY_CLICKHOUSE_URL"), std::env::var("AWZY_CLICKHOUSE_PASSWORD")) {
            (Ok(url), Ok(password)) => Some(ClickHouseConfig {
                url,
                user: std::env::var("AWZY_CLICKHOUSE_USER").unwrap_or_else(|_| "awzy_reporter".into()),
                password,
            }),
            _ => None,
        };
        let kafka = awzy_kafka::Brokers::from_env("AWZY_KAFKA_BROKERS")?;
        Ok(Self { clickhouse, kafka })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DataMetrics {
    pub clickhouse_disk_used_percent: Option<f64>,
    pub kafka_lag: Option<u64>,
}

/// One consumer's position in one partition. Negative: it hasn't read from it yet.
#[derive(Debug, Clone, PartialEq, Eq, clickhouse::Row, serde::Deserialize)]
struct Position {
    topic: String,
    partition: i32,
    offset: i64,
}

pub struct DataSources {
    clickhouse: Option<clickhouse::Client>,
    brokers: Option<awzy_kafka::Brokers>,
    kafka: Option<KafkaClient>,
}

impl DataSources {
    pub fn new(config: DataConfig) -> Self {
        let clickhouse = config
            .clickhouse
            .map(|c| clickhouse::Client::default().with_url(c.url).with_user(c.user).with_password(c.password));
        Self { clickhouse, brokers: config.kafka, kafka: None }
    }

    pub async fn collect(&mut self) -> DataMetrics {
        let Some(ch) = self.clickhouse.clone() else { return DataMetrics::default() };
        let disk = match tokio::time::timeout(TIMEOUT, disk_used_percent(&ch)).await {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "couldn't read ClickHouse disk use");
                None
            }
            Err(_) => None,
        };
        let lag = match tokio::time::timeout(TIMEOUT * 2, self.kafka_lag(&ch)).await {
            Ok(Ok(lag)) => lag,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "couldn't measure Kafka lag");
                self.kafka = None; // reconnect next time
                None
            }
            Err(_) => None,
        };
        DataMetrics { clickhouse_disk_used_percent: disk, kafka_lag: lag }
    }

    async fn kafka_lag(&mut self, ch: &clickhouse::Client) -> anyhow::Result<Option<u64>> {
        let Some(brokers) = &self.brokers else {
            return Ok(None);
        };
        let positions = consumer_positions(ch).await?;
        if positions.is_empty() {
            // No consumer is running, so there's nothing to measure lag against.
            return Ok(None);
        }
        if self.kafka.is_none() {
            self.kafka = Some(brokers.connect().await?);
        }
        let kafka = self.kafka.as_ref().expect("connected above");
        let mut ends = BTreeMap::new();
        for p in &positions {
            let client = kafka.partition_client(p.topic.clone(), p.partition, UnknownTopicHandling::Error).await?;
            let latest = client.get_offset(OffsetAt::Latest).await?;
            let earliest = if p.offset < 0 { client.get_offset(OffsetAt::Earliest).await? } else { 0 };
            ends.insert((p.topic.clone(), p.partition), (earliest, latest));
        }
        Ok(Some(lag(&positions, &ends)))
    }
}

/// The most-used disk's share, 0 to 100.
async fn disk_used_percent(ch: &clickhouse::Client) -> anyhow::Result<Option<f64>> {
    let used: Option<f64> = ch
        .query(
            "SELECT max((total_space - free_space) / total_space) * 100 FROM system.disks WHERE total_space > 0
             HAVING count() > 0",
        )
        .fetch_optional()
        .await?;
    Ok(used.map(|u| u.clamp(0.0, 100.0)))
}

async fn consumer_positions(ch: &clickhouse::Client) -> anyhow::Result<Vec<Position>> {
    Ok(ch
        .query(
            "SELECT a.1 AS topic, toInt32(a.2) AS partition, a.3 AS offset
             FROM system.kafka_consumers
             ARRAY JOIN arrayZip(assignments.topic, assignments.partition_id, assignments.current_offset) AS a",
        )
        .fetch_all()
        .await?)
}

/// Records not yet read, summed over every assigned partition. A partition the consumer hasn't
/// read from counts from its earliest offset.
fn lag(positions: &[Position], ends: &BTreeMap<(String, i32), (i64, i64)>) -> u64 {
    positions
        .iter()
        .filter_map(|p| {
            let (earliest, latest) = ends.get(&(p.topic.clone(), p.partition))?;
            let from = if p.offset < 0 { *earliest } else { p.offset };
            Some(u64::try_from(latest - from).unwrap_or(0))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(topic: &str, partition: i32, offset: i64) -> Position {
        Position { topic: topic.into(), partition, offset }
    }

    #[test]
    fn lag_sums_what_each_consumer_has_left_to_read() {
        let positions = [at("decisions", 0, 90), at("decisions", 1, -1001), at("events", 0, 50)];
        let ends = BTreeMap::from([
            (("decisions".to_string(), 0), (0, 100)),
            (("decisions".to_string(), 1), (20, 25)),
            (("events".to_string(), 0), (0, 40)),
        ]);
        // 10 behind, 5 unread from the earliest, and a position past the end counts as caught up.
        assert_eq!(lag(&positions, &ends), 15);
    }

    /// With --config=local-stack: reads the local ClickHouse as the admin user (the reporter's
    /// own user is tested in authz/jobs/clickhouse-migrate) and the local Kafka.
    #[tokio::test]
    async fn measures_disk_use_and_lag_from_the_local_stack() {
        let (Ok(url), Ok(brokers)) =
            (std::env::var("AWZY_TEST_CLICKHOUSE_URL"), std::env::var("AWZY_TEST_KAFKA_BROKERS"))
        else {
            eprintln!("skipping: local-stack environment is not set");
            return;
        };
        let mut sources = DataSources::new(DataConfig {
            clickhouse: Some(ClickHouseConfig { url, user: "awzy".into(), password: "awzy".into() }),
            kafka: Some(awzy_kafka::Brokers::plaintext(&brokers)),
        });
        let m = sources.collect().await;
        let disk = m.clickhouse_disk_used_percent.expect("disk use");
        assert!((0.0..=100.0).contains(&disk), "{disk}");
        // Lag is measured whenever a ClickHouse Kafka consumer is running in the local stack.
        let ch = sources.clickhouse.clone().unwrap();
        if !consumer_positions(&ch).await.unwrap().is_empty() {
            assert!(m.kafka_lag.is_some(), "a consumer is running, so lag is measured");
        }
    }
}
