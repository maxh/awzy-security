use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-clickhouse-migrate");
    let env = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
    let server = clickhouse::Client::default()
        .with_url(env("AWZY_CLICKHOUSE_URL")?)
        .with_user(env("AWZY_CLICKHOUSE_USER")?)
        .with_password(env("AWZY_CLICKHOUSE_PASSWORD")?);
    let database = env("AWZY_CLICKHOUSE_DATABASE")?;
    // `repair disk-cleanup`: a repair the client portal started (ADR 0102), not a migration.
    if std::env::args().skip(1).collect::<Vec<_>>() == ["repair", "disk-cleanup"] {
        let done = awzy_clickhouse::repair::disk_cleanup(&server.with_database(&database)).await?;
        tracing::info!(detached_parts = done.detached_parts, system_logs = ?done.system_logs, ttls = ?done.ttls_applied, "disk cleanup done");
        return Ok(());
    }
    // A new install's ClickHouse has no database yet (the local stack's image makes one).
    server.query(&format!("CREATE DATABASE IF NOT EXISTS {database}")).execute().await?;
    let client = server.with_database(&database);
    let settings = awzy_clickhouse::Settings {
        kafka_brokers: env("AWZY_CLICKHOUSE_KAFKA_BROKERS")?,
        decisions_topic: std::env::var("AWZY_DECISIONS_TOPIC").unwrap_or_else(|_| "awzy.decisions.v1".into()),
        events_topic: std::env::var("AWZY_EVENTS_TOPIC").unwrap_or_else(|_| "awzy.events.v1".into()),
        consumer_group: std::env::var("AWZY_CLICKHOUSE_CONSUMER_GROUP").unwrap_or_else(|_| "awzy-clickhouse".into()),
    };
    // Best effort: locally the brokers are named as ClickHouse reaches them, inside Docker.
    // AWZY_KAFKA_TLS says whether to reach them over TLS, as it does for the services.
    let topic_brokers = awzy_kafka::Brokers::from_env("AWZY_CLICKHOUSE_KAFKA_BROKERS")?
        .context("AWZY_CLICKHOUSE_KAFKA_BROKERS is not set")?;
    let topics = [settings.decisions_topic.as_str(), settings.events_topic.as_str()];
    match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        awzy_clickhouse::ensure_topics(&topic_brokers, &topics),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "couldn't create the Kafka topics; their producers will"),
        Err(_) => tracing::warn!("couldn't reach Kafka to create the topics; their producers will"),
    }
    let applied = awzy_clickhouse::migrate(&client, &settings).await?;
    tracing::info!(?applied, "ClickHouse schema is current");
    awzy_clickhouse::ensure_queue_brokers(&client, &settings.kafka_brokers).await?;
    // How long decisions and events are kept; unset or 0 keeps them all.
    let days = std::env::var("AWZY_CLICKHOUSE_RETENTION_DAYS").ok().map(|d| d.parse::<u32>()).transpose()?;
    awzy_clickhouse::ensure_retention(&client, days.filter(|d| *d > 0)).await?;
    // ADR 0042: the reporter's read-only user, when the install gives it a password.
    if let Ok(password) = std::env::var("AWZY_CLICKHOUSE_REPORTER_PASSWORD") {
        awzy_clickhouse::ensure_reporter_user(&client, awzy_clickhouse::REPORTER_USER, &password).await?;
    }
    // ADR 0056: awzy-reasoner's read-only user, likewise.
    if let Ok(password) = std::env::var("AWZY_CLICKHOUSE_REASONER_PASSWORD") {
        awzy_clickhouse::ensure_reasoner_user(&client, &database, awzy_clickhouse::REASONER_USER, &password).await?;
    }
    Ok(())
}
