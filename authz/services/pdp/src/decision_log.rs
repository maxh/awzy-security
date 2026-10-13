//! Sends every decision to Kafka, where ClickHouse picks it up (see authz/jobs/clickhouse-migrate).
//!
//! Recording is fire-and-forget: records go into a bounded buffer and a background task
//! batches them to the topic, partitioned by principal so one principal's decisions stay in order.
//! When Kafka is slow or down and the buffer fills, records are dropped rather than slowing
//! decisions down. Every dropped record is counted in `awzy_pdp_decisions_dropped_total`, which
//! an alert watches, because the audit trail is missing it.
//!
//! A PDP doesn't wait for Kafka to start (#400): checks don't need it, and a PDP that can't start
//! during a Kafka outage can't be replaced or scaled out. Records buffer from the first check
//! while the sender connects in the background.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use awzy_model::DecisionRecord;
use rskafka::{
    client::{
        error::{Error as KafkaError, ProtocolError},
        partition::{Compression, PartitionClient, UnknownTopicHandling},
    },
    record::Record,
};
use tokio::sync::mpsc;

pub const DEFAULT_DECISIONS_TOPIC: &str = "awzy.decisions.v1";

/// Records held while Kafka catches up. About 10 MB at typical record sizes.
const BUFFER: usize = 20_000;
const MAX_BATCH: usize = 1_000;
const ATTEMPTS: u32 = 5;
const TOPIC_PARTITIONS: i32 = 6;

pub struct DecisionLog {
    tx: mpsc::Sender<DecisionRecord>,
    dropped: AtomicU64,
}

impl DecisionLog {
    /// Connects, creates the topic if it does not exist, and starts the background sender.
    pub async fn connect_kafka(brokers: &awzy_kafka::Brokers, topic: &str) -> anyhow::Result<Arc<Self>> {
        let clients = connect_partitions(brokers, topic).await?;
        let (tx, rx) = mpsc::channel(BUFFER);
        tokio::spawn(send_batches(rx, clients));
        Ok(Arc::new(Self::sending_to(tx)))
    }

    /// Starts at once and connects in the background, retrying until Kafka answers. Records
    /// buffer meanwhile, and are dropped and counted once the buffer is full.
    pub fn start_kafka(brokers: awzy_kafka::Brokers, topic: String) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(BUFFER);
        tokio::spawn(async move {
            let clients = until_connected("decisions", || connect_partitions(&brokers, &topic)).await;
            send_batches(rx, clients).await;
        });
        Arc::new(Self::sending_to(tx))
    }
}

/// The topic's partition clients, creating the topic if it does not exist.
async fn connect_partitions(brokers: &awzy_kafka::Brokers, topic: &str) -> anyhow::Result<Vec<PartitionClient>> {
    let client = brokers.connect().await?;
    let expected = match client.controller_client()?.create_topic(topic, TOPIC_PARTITIONS, 1, 5_000).await {
        Ok(()) => {
            tracing::info!(topic, "created decisions topic");
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
    Ok(clients)
}

/// Retries `connect` with backoff, up to 30 s apart, until it succeeds.
pub(crate) async fn until_connected<T, F, Fut>(log: &str, mut connect: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut wait = Duration::from_secs(1);
    loop {
        match connect().await {
            Ok(connected) => {
                tracing::info!(log, "connected to Kafka");
                return connected;
            }
            Err(error) => {
                tracing::warn!(%error, log, retry_in_secs = wait.as_secs(), "can't reach Kafka; records buffer until it answers");
                tokio::time::sleep(wait).await;
                wait = (wait * 2).min(Duration::from_secs(30));
            }
        }
    }
}

impl DecisionLog {
    fn sending_to(tx: mpsc::Sender<DecisionRecord>) -> Self {
        Self { tx, dropped: AtomicU64::new(0) }
    }

    pub fn record(&self, record: DecisionRecord) {
        if self.tx.try_send(record).is_err() {
            crate::metrics::DECISIONS_DROPPED_BUFFER_FULL.inc();
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            if dropped.is_power_of_two() {
                tracing::warn!(dropped, "decision log buffer full; dropping decision records");
            }
        }
    }

    /// Records dropped since startup because the buffer was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

async fn send_batches(mut rx: mpsc::Receiver<DecisionRecord>, partitions: Vec<PartitionClient>) {
    let mut batch = Vec::with_capacity(MAX_BATCH);
    while rx.recv_many(&mut batch, MAX_BATCH).await > 0 {
        let mut by_partition: Vec<Vec<Record>> = vec![Vec::new(); partitions.len()];
        for r in batch.drain(..) {
            by_partition[partition_for(&r, partitions.len())].push(to_kafka(&r));
        }
        for (client, records) in partitions.iter().zip(by_partition) {
            if !records.is_empty() {
                produce_with_retry(client, records).await;
            }
        }
    }
}

async fn produce_with_retry(client: &PartitionClient, records: Vec<Record>) {
    let count = records.len();
    for attempt in 1..=ATTEMPTS {
        match client.produce(records.clone(), Compression::NoCompression).await {
            Ok(_) => return,
            Err(error) if attempt < ATTEMPTS => {
                tracing::warn!(%error, attempt, "producing decision records failed; retrying");
                tokio::time::sleep(Duration::from_millis(100 * 2u64.pow(attempt))).await;
            }
            Err(error) => {
                crate::metrics::DECISIONS_DROPPED_PRODUCE_FAILED.inc_by(count as u64);
                tracing::error!(%error, count, "dropping decision records after retries");
            }
        }
    }
}

fn to_kafka(r: &DecisionRecord) -> Record {
    Record {
        key: Some(r.principal_id.clone().into_bytes()),
        value: Some(serde_json::to_vec(r).expect("DecisionRecord serializes")),
        headers: BTreeMap::new(),
        timestamp: r.at,
    }
}

/// Stable across processes and releases (FNV-1a), unlike std's hasher.
fn partition_for(r: &DecisionRecord, partitions: usize) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in r.principal_type.bytes().chain([0]).chain(r.principal_id.bytes()) {
        hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    (hash % partitions as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_model::DecisionId;

    fn record(principal: &str) -> DecisionRecord {
        DecisionRecord {
            id: DecisionId::new(),
            at: chrono::Utc::now(),
            principal_type: "Human".into(),
            principal_id: principal.into(),
            on_behalf_of_type: String::new(),
            on_behalf_of_id: String::new(),
            human_present: true,
            action: "read".into(),
            resource_type: "Resource".into(),
            resource_id: "doc".into(),
            verdict: "allow".into(),
            mode: awzy_model::EnforcementMode::Monitor,
            challenge: String::new(),
            policies: vec![],
            on_behalf_of_verdict: String::new(),
            on_behalf_of_policies: vec![],
            policy_version: 0,
            context: "{}".into(),
            resource_attributes_from: String::new(),
            work_item_id: String::new(),
            stale_inputs: vec![],
        }
    }

    #[test]
    fn same_subject_same_partition() {
        let a = partition_for(&record("alice"), 6);
        assert_eq!(a, partition_for(&record("alice"), 6));
        assert!(a < 6);
    }

    #[test]
    fn a_full_buffer_drops_and_counts_records() {
        let (tx, _rx) = mpsc::channel(1);
        let log = DecisionLog::sending_to(tx);
        let before = crate::metrics::DECISIONS_DROPPED_BUFFER_FULL.get();
        log.record(record("alice"));
        log.record(record("alice"));
        log.record(record("alice"));
        assert_eq!(log.dropped(), 2);
        // Other tests may drop records too, so the process-wide counter is at least this.
        assert!(crate::metrics::DECISIONS_DROPPED_BUFFER_FULL.get() >= before + 2);
    }

    #[tokio::test]
    async fn a_pdp_starts_while_kafka_is_down_and_buffers_its_records() {
        let log = DecisionLog::start_kafka(awzy_kafka::Brokers::plaintext("127.0.0.1:1"), "unreachable".into());
        for _ in 0..3 {
            log.record(record("alice"));
        }
        assert_eq!(log.dropped(), 0, "held until Kafka answers, not dropped");
    }

    /// Runs with --config=local-stack: records made before the background connection is up reach
    /// the topic, read back in order.
    #[tokio::test]
    async fn records_reach_the_topic() {
        let Ok(brokers) = std::env::var("AWZY_TEST_KAFKA_BROKERS") else {
            eprintln!("skipping: AWZY_TEST_KAFKA_BROKERS is not set");
            return;
        };
        let brokers = awzy_kafka::Brokers::plaintext(&brokers);
        let topic = format!("test.decisions.{}", uuid::Uuid::now_v7().simple());
        let log = DecisionLog::start_kafka(brokers.clone(), topic.clone());
        let records: Vec<DecisionRecord> = (0..3).map(|_| record("alice")).collect();
        let ids: Vec<DecisionId> = records.iter().map(|r| r.id).collect();
        for r in records {
            log.record(r);
        }

        let client = brokers.connect().await.unwrap();
        let partition = partition_for(&record("alice"), TOPIC_PARTITIONS as usize) as i32;
        let reader = client.partition_client(topic, partition, UnknownTopicHandling::Retry).await.unwrap();
        let mut seen = Vec::new();
        for _ in 0..50 {
            let (records, _) = reader.fetch_records(seen.len() as i64, 1..1_000_000, 200).await.unwrap();
            for r in records {
                let d: DecisionRecord = serde_json::from_slice(r.record.value.as_deref().unwrap()).unwrap();
                seen.push(d.id);
            }
            if seen.len() == ids.len() {
                break;
            }
        }
        assert_eq!(seen, ids);
        assert_eq!(log.dropped(), 0);
    }
}
