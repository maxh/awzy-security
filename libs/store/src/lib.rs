//! Connections to the client-side data stores: Postgres, ClickHouse, Kafka, Valkey.
//!
//! Visible only to client-side code (see //tools:client_side), so portals/corp cannot link it.

/// Connection settings, read from the environment the Helm chart provides.
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub postgres_url: String,
    pub clickhouse_url: String,
    pub kafka_brokers: String,
    pub valkey_url: String,
}

impl StoreConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |k: &str| std::env::var(k).map_err(|_| anyhow::anyhow!("{k} is not set"));
        Ok(Self {
            postgres_url: var("AWZY_POSTGRES_URL")?,
            clickhouse_url: var("AWZY_CLICKHOUSE_URL")?,
            kafka_brokers: var("AWZY_KAFKA_BROKERS")?,
            valkey_url: var("AWZY_VALKEY_URL")?,
        })
    }
}

pub mod changes;
pub mod migrations;
