//! Sends every plan to Kafka, where ClickHouse keeps it in `plans` (ADR 0113), as the decision log
//! does for decisions: fire-and-forget, batched, dropped and counted (`awzy_pdp_plans_dropped_total`)
//! when Kafka can't keep up.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use awzy_model::PlanRecord;
use rskafka::{
    client::{
        error::{Error as KafkaError, ProtocolError},
        partition::{Compression, PartitionClient, UnknownTopicHandling},
    },
    record::Record,
};
use tokio::sync::mpsc;

pub const DEFAULT_PLANS_TOPIC: &str = "awzy.plans.v1";

const BUFFER: usize = 5_000;
const MAX_BATCH: usize = 500;
const PARTITIONS: i32 = 6;

pub struct PlanLog {
    tx: mpsc::Sender<PlanRecord>,
}

impl PlanLog {
    /// Connects, creates the topic if it does not exist, and starts the background sender.
    pub async fn connect_kafka(brokers: &awzy_kafka::Brokers, topic: &str) -> anyhow::Result<Arc<Self>> {
        let client = brokers.connect().await?;
        match client.controller_client()?.create_topic(topic, PARTITIONS, 1, 5_000).await {
            Ok(()) => tracing::info!(topic, "created plans topic"),
            Err(KafkaError::ServerError { protocol_error: ProtocolError::TopicAlreadyExists, .. }) => {}
            Err(e) => return Err(e.into()),
        }
        let partition = client.partition_client(topic.to_string(), 0, UnknownTopicHandling::Retry).await?;
        let (tx, rx) = mpsc::channel(BUFFER);
        tokio::spawn(send(rx, partition));
        Ok(Arc::new(Self { tx }))
    }

    pub fn record(&self, record: PlanRecord) {
        if self.tx.try_send(record).is_err() {
            crate::metrics::PLANS_DROPPED_BUFFER_FULL.inc();
            tracing::warn!("plan log buffer full; dropping a plan record");
        }
    }
}

async fn send(mut rx: mpsc::Receiver<PlanRecord>, partition: PartitionClient) {
    let mut batch = Vec::with_capacity(MAX_BATCH);
    while rx.recv_many(&mut batch, MAX_BATCH).await > 0 {
        let records: Vec<Record> = batch
            .drain(..)
            .filter_map(|r| {
                Some(Record {
                    key: Some(r.principal_id.clone().into_bytes()),
                    value: Some(serde_json::to_vec(&r).ok()?),
                    headers: BTreeMap::new(),
                    timestamp: r.at,
                })
            })
            .collect();
        for attempt in 1..=3u32 {
            match partition.produce(records.clone(), Compression::NoCompression).await {
                Ok(_) => break,
                Err(error) if attempt < 3 => {
                    tracing::warn!(%error, attempt, "producing plan records failed; retrying");
                    tokio::time::sleep(Duration::from_millis(200 * u64::from(attempt))).await;
                }
                Err(error) => {
                    crate::metrics::PLANS_DROPPED_PRODUCE_FAILED.inc_by(records.len() as u64);
                    tracing::error!(%error, count = records.len(), "dropping plan records after retries");
                }
            }
        }
    }
}
