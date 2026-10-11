//! Sends normalized events to Kafka (`awzy.events.v1`), where ClickHouse picks them up (see
//! authz/jobs/clickhouse-migrate).
//!
//! Unlike the PDP's decision log, which drops records rather than slow a decision, intake
//! answers only after Kafka has every event in the request: a log shipper retries on an error,
//! so nothing is lost when Kafka is down, at the cost of occasional duplicates. Events are
//! partitioned by subject so one principal's events stay in order.

use std::{collections::BTreeMap, time::Duration};

use awzy_model::EventRecord;
use rskafka::{
    client::{
        error::{Error as KafkaError, ProtocolError},
        partition::{Compression, PartitionClient, UnknownTopicHandling},
        ClientBuilder,
    },
    record::Record,
};

pub const DEFAULT_EVENTS_TOPIC: &str = "awzy.events.v1";

const TOPIC_PARTITIONS: i32 = 6;

pub struct EventLog {
    partitions: Vec<PartitionClient>,
}

impl EventLog {
    /// Connects and creates the topic if it does not exist.
    pub async fn connect_kafka(brokers: Vec<String>, topic: &str) -> anyhow::Result<Self> {
        let client = ClientBuilder::new(brokers).build().await?;
        let expected = match client.controller_client()?.create_topic(topic, TOPIC_PARTITIONS, 1, 5_000).await {
            Ok(()) => {
                tracing::info!(topic, "created events topic");
                TOPIC_PARTITIONS as usize
            }
            Err(KafkaError::ServerError { protocol_error: ProtocolError::TopicAlreadyExists, .. }) => 1,
            Err(e) => return Err(e.into()),
        };
        // Metadata for a new topic can lag its creation; wait until every partition shows up.
        let mut partitions = Default::default();
        for _ in 0..50 {
            partitions = client
                .list_topics()
                .await?
                .into_iter()
                .find(|t| t.name == topic)
                .map(|t| t.partitions)
                .unwrap_or_default();
            if partitions.len() >= expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        anyhow::ensure!(
            partitions.len() >= expected,
            "topic {topic} has {} partitions after waiting",
            partitions.len()
        );
        let mut clients = Vec::with_capacity(partitions.len());
        for p in partitions {
            clients.push(client.partition_client(topic.to_string(), p, UnknownTopicHandling::Retry).await?);
        }
        Ok(Self { partitions: clients })
    }

    /// Returns once Kafka has acknowledged every event.
    pub async fn send(&self, events: &[EventRecord]) -> Result<(), KafkaError> {
        let mut by_partition: Vec<Vec<Record>> = vec![Vec::new(); self.partitions.len()];
        for e in events {
            by_partition[partition_for(e, self.partitions.len())].push(to_kafka(e));
        }
        for (client, records) in self.partitions.iter().zip(by_partition) {
            if !records.is_empty() {
                client.produce(records, Compression::NoCompression).await?;
            }
        }
        Ok(())
    }
}

fn to_kafka(e: &EventRecord) -> Record {
    Record {
        key: Some(e.subject_id.clone().into_bytes()),
        value: Some(serde_json::to_vec(e).expect("EventRecord serializes")),
        headers: BTreeMap::new(),
        timestamp: e.at,
    }
}

/// Stable across processes and releases (FNV-1a), unlike std's hasher.
fn partition_for(e: &EventRecord, partitions: usize) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in e.subject_type.bytes().chain([0]).chain(e.subject_id.bytes()) {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    (hash % partitions as u64) as usize
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use awzy_model::EventId;

    pub(crate) fn event(subject: &str) -> EventRecord {
        let now = chrono::Utc::now();
        EventRecord {
            id: EventId::new(),
            at: now,
            received_at: now,
            source: "app".into(),
            system: "jira".into(),
            subject_type: "Human".into(),
            subject_id: subject.into(),
            on_behalf_of_type: String::new(),
            on_behalf_of_id: String::new(),
            action: "read".into(),
            resource_type: "Ticket".into(),
            resource_id: "ENG-1".into(),
            outcome: "success".into(),
            source_ip: String::new(),
            attributes: "{}".into(),
        }
    }

    #[test]
    fn one_subjects_events_share_a_partition() {
        let a = partition_for(&event("alice"), 6);
        assert_eq!(a, partition_for(&event("alice"), 6));
        assert!(a < 6);
    }

    /// Runs with --config=local-stack: sends through EventLog, reads the events back.
    #[tokio::test]
    async fn sent_events_are_on_the_topic() {
        let Ok(brokers) = std::env::var("AWZY_TEST_KAFKA_BROKERS") else {
            eprintln!("skipping: AWZY_TEST_KAFKA_BROKERS is not set");
            return;
        };
        let brokers: Vec<String> = brokers.split(',').map(str::to_string).collect();
        let topic = format!("test.events.{}", uuid::Uuid::now_v7().simple());
        let log = EventLog::connect_kafka(brokers.clone(), &topic).await.unwrap();
        let events: Vec<EventRecord> = (0..3).map(|_| event("alice")).collect();
        log.send(&events).await.unwrap();

        // Acknowledged means readable: no waiting needed.
        let client = ClientBuilder::new(brokers).build().await.unwrap();
        let partition = partition_for(&event("alice"), TOPIC_PARTITIONS as usize) as i32;
        let reader = client.partition_client(topic, partition, UnknownTopicHandling::Retry).await.unwrap();
        let (records, _) = reader.fetch_records(0, 1..1_000_000, 1_000).await.unwrap();
        // Times travel in milliseconds, so compare IDs rather than whole records.
        let seen: Vec<EventId> = records
            .iter()
            .map(|r| serde_json::from_slice::<EventRecord>(r.record.value.as_deref().unwrap()).unwrap().id)
            .collect();
        assert_eq!(seen, events.iter().map(|e| e.id).collect::<Vec<_>>());
    }
}
