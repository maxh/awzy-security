//! Repairs a client admin starts from the client portal's Status page, each as a Job the chart
//! defines (ADR 0102). They act only on what's safe to lose: awzy's data replays from Kafka
//! (ADR 0006), and nothing here touches a table's live parts.

use clickhouse::Client;

/// What a disk cleanup freed, for the Job's log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cleanup {
    pub detached_parts: usize,
    pub system_logs: Vec<String>,
    pub ttls_applied: Vec<String>,
}

/// Frees disk without losing awzy's data: drops parts ClickHouse detached (broken or left over from
/// a failed merge), empties ClickHouse's own logs of queries and metrics, which grow without
/// bound, and applies `decisions` and `events` retention now rather than at the next merge.
pub async fn disk_cleanup(client: &Client) -> anyhow::Result<Cleanup> {
    let mut done = Cleanup::default();

    // `<table>\t<part>` rather than a tuple, so plain strings come back.
    let detached: Vec<String> = client
        .query("SELECT concat(table, '\t', name) FROM system.detached_parts WHERE database = currentDatabase()")
        .fetch_all()
        .await?;
    for row in &detached {
        let Some((table, part)) = row.split_once('\t') else { continue };
        client
            .query(&format!("ALTER TABLE `{table}` DROP DETACHED PART '{}'", part.replace('\'', "\\'")))
            .with_setting("allow_drop_detached", "1")
            .execute()
            .await?;
        done.detached_parts += 1;
    }

    let logs: Vec<String> = client
        .query(
            "SELECT name FROM system.tables WHERE database = 'system' AND name LIKE '%\\_log' \
             AND engine LIKE '%MergeTree' ORDER BY name",
        )
        .fetch_all()
        .await?;
    for log in logs {
        client.query(&format!("TRUNCATE TABLE system.`{log}`")).execute().await?;
        done.system_logs.push(log);
    }

    for table in crate::RETAINED {
        let engine: Vec<String> = client
            .query("SELECT engine_full FROM system.tables WHERE database = currentDatabase() AND name = ?")
            .bind(*table)
            .fetch_all()
            .await?;
        if engine.first().is_some_and(|e| e.contains(" TTL ")) {
            client.query(&format!("ALTER TABLE {table} MATERIALIZE TTL")).execute().await?;
            done.ttls_applied.push((*table).to_string());
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With --config=local-stack: a cleanup runs on a migrated database and empties the system
    /// logs it finds, and running it again is harmless.
    #[tokio::test]
    async fn a_disk_cleanup_runs_and_can_run_again() {
        let Ok(url) = std::env::var("AWZY_TEST_CLICKHOUSE_URL") else {
            eprintln!("skipping: AWZY_TEST_CLICKHOUSE_URL is not set");
            return;
        };
        let client = Client::default().with_url(&url).with_user("awzy").with_password("awzy").with_database("awzy");
        let first = disk_cleanup(&client).await.unwrap();
        assert!(first.system_logs.iter().all(|l| l.ends_with("_log")), "{:?}", first.system_logs);
        disk_cleanup(&client).await.unwrap();
    }
}
