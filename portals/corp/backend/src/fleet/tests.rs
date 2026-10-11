use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::*;

pub(crate) async fn test_store() -> Option<FleetStore> {
    let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
        eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
        return None;
    };
    let schema = format!("test_{}", Uuid::now_v7().simple());
    let admin = PgPool::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
    let store = FleetStore::new(pool);
    store.migrate().await.unwrap();
    Some(store)
}

pub(crate) fn new_install(client: &str, environment: &str) -> NewInstall {
    NewInstall {
        client: client.into(),
        environment: environment.into(),
        cloud: "aws".into(),
        region: "us-east-2".into(),
        channel: Channel::Fast,
        pinned_version: None,
    }
}

#[test]
fn versions_compare_by_number() {
    use std::cmp::Ordering::*;
    assert_eq!(compare_versions("0.10.0", "0.9.0"), Greater);
    assert_eq!(compare_versions("0.5.0-rc.2", "0.5.0-rc.10"), Less);
    assert_eq!(compare_versions("1.0.0", "1.0.0"), Equal);
}

#[test]
fn registration_input_is_checked() {
    let mut bad = new_install("Acme Corp", "prod");
    assert!(bad.validate().is_err(), "client names are slugs");
    bad = new_install("acme", "prod");
    bad.channel = Channel::Pinned;
    assert!(bad.validate().is_err(), "pinned needs a version");
    bad.pinned_version = Some("0.4.1".into());
    assert!(bad.validate().is_err(), "a pin names a version CI publishes");
    bad.pinned_version = Some("0.4.1-g000000000004".into());
    assert!(bad.validate().is_ok());
    bad.cloud = "oracle".into();
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn registers_once_per_client_environment_and_authenticates_tokens() {
    let Some(store) = test_store().await else { return };
    let (install, token) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
    assert!(token.starts_with("awzyrt_"));
    assert_eq!(store.authenticate(&token).await.unwrap(), Some(install.id));
    assert_eq!(store.authenticate("awzyrt_wrong").await.unwrap(), None);
    assert!(matches!(store.register(&new_install("acme", "prod"), "max").await, Err(RegisterError::Exists { .. })));
    assert_eq!(install.health(Utc::now()), Health::NeverReported);
}

#[tokio::test]
async fn a_retired_install_stops_authenticating_and_frees_its_name() {
    let Some(store) = test_store().await else { return };
    let url = std::env::var("AWZY_TEST_POSTGRES_URL").unwrap();
    let registry_schema = format!("test_registry_{}", Uuid::now_v7().simple());
    let pull_tokens = awzy_registry::tokens::TokenStore::connect(&url, &registry_schema).await.unwrap();
    let store = store.with_pull_tokens(pull_tokens);

    let (old, reporting) = store.register(&new_install("acme", "demo"), "max").await.unwrap();
    let pull = store.issue_pull_token(old.id, "max").await.unwrap().expect("this store issues pull tokens");
    let registry = store.pull_tokens.clone().unwrap();
    assert!(registry.check(old.id, &pull).await.unwrap());

    let retired = store.retire(old.id, "max").await.unwrap().expect("it was active");
    assert_eq!(retired.retired_by.as_deref(), Some("max"));
    assert_eq!(store.authenticate(&reporting).await.unwrap(), None, "its reporting token stops working");
    assert!(!registry.check(old.id, &pull).await.unwrap(), "and so does its pull token");
    assert!(store.retire(old.id, "max").await.unwrap().is_none(), "retiring twice changes nothing");
    assert!(store.all_installs().await.unwrap().iter().all(|i| i.id != old.id), "it leaves the fleet");
    assert!(store.get(old.id).await.unwrap().is_some(), "but its record stays");

    let (new, _) = store.register(&new_install("acme", "demo"), "max").await.unwrap();
    assert_ne!(new.id, old.id, "the same client and environment register again, as a new install");
}

#[tokio::test]
async fn an_install_moves_between_channels_and_pins() {
    let Some(store) = test_store().await else { return };
    let (install, _) = store.register(&new_install("acme", "staging"), "max").await.unwrap();
    let pinned = store.set_release(install.id, Channel::Pinned, Some("0.4.1-g000000000004")).await.unwrap().unwrap();
    assert_eq!((pinned.channel, pinned.pinned_version.as_deref()), (Channel::Pinned, Some("0.4.1-g000000000004")));
    let fast = store.set_release(install.id, Channel::Fast, None).await.unwrap().unwrap();
    assert_eq!((fast.channel, fast.pinned_version), (Channel::Fast, None));
    assert!(matches!(store.set_release(install.id, Channel::Pinned, None).await, Err(RegisterError::Invalid(_))));
    store.retire(install.id, "max").await.unwrap();
    assert!(store.set_release(install.id, Channel::Stable, None).await.unwrap().is_none(), "not once it's retired");
}

fn beat(install: InstallId, p99: f64, dps: f64, ready: u32) -> Heartbeat {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "install_qid": install, "sent_at": Utc::now(), "chart_version": "0.5.0",
        "services": [{ "service": "awzy-pdp", "version": "0.5.0", "image_digest": null, "replicas_desired": 2,
                       "replicas_ready": ready, "restarts_24h": 0, "ready": true }],
        "metrics": { "decisions_per_second": dps, "pdp_p99_ms": p99 },
    }))
    .unwrap()
}

#[tokio::test]
async fn chart_samples_show_the_worst_of_each_bucket_and_leave_gaps() {
    let Some(store) = test_store().await else { return };
    let (install, _) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
    let now: DateTime<Utc> = "2026-10-09T12:00:00Z".parse().unwrap();
    let at = |minutes_ago: i64| now - Duration::minutes(minutes_ago);
    // One 5-minute bucket with two heartbeats, an hour of silence, then one more.
    store.record_heartbeat(&beat(install.id, 2.0, 40.0, 2), at(64)).await.unwrap();
    store.record_heartbeat(&beat(install.id, 9.0, 60.0, 1), at(63)).await.unwrap();
    store.record_heartbeat(&beat(install.id, 1.5, 50.0, 2), at(2)).await.unwrap();

    let samples = store.samples(install.id, Window::Day, now).await.unwrap();
    assert_eq!(samples.len(), 2, "the silent hour has no buckets: {samples:?}");
    assert_eq!(samples[0].at, "2026-10-09T10:55:00Z".parse::<DateTime<Utc>>().unwrap());
    assert_eq!(samples[0].pdp_p99_ms, Some(9.0), "worst p99 in the bucket");
    assert_eq!(samples[0].decisions_per_second, Some(50.0), "average throughput");
    assert!(samples[0].degraded, "one PDP replica wasn't ready");
    assert!(!samples[1].degraded);
}

#[tokio::test]
async fn samples_older_than_the_retention_are_dropped() {
    let Some(store) = test_store().await else { return };
    let (install, _) = store.register(&new_install("acme", "prod"), "max").await.unwrap();
    let now: DateTime<Utc> = "2026-10-09T12:00:00Z".parse().unwrap();
    store.record_heartbeat(&beat(install.id, 1.0, 1.0, 2), now - Duration::days(31)).await.unwrap();
    store.record_heartbeat(&beat(install.id, 1.0, 1.0, 2), now).await.unwrap();
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM heartbeat_samples").fetch_one(&store.pool).await.unwrap();
    assert_eq!(kept, 1);
}

#[test]
fn only_versions_ci_publishes_can_be_named() {
    assert!(is_chart_version("0.1.0-g0123456789ab"));
    assert!(!is_chart_version("0.1.0"));
    assert!(!is_chart_version("0.1.0-g0123"));
    assert!(!is_chart_version("0.1.0-gXYZ456789abc"));
    assert!(!is_chart_version(">=0.0.0-0"));
}

#[tokio::test]
async fn a_channel_names_its_latest_version_and_keeps_its_history() {
    let Some(store) = test_store().await else { return };
    assert!(store.channel_versions().await.unwrap().is_empty());
    store.set_channel(Channel::Fast, "0.1.0-g000000000001", "github-actions").await.unwrap();
    store.set_channel(Channel::Stable, "0.1.0-g000000000001", "alice").await.unwrap();
    store.set_channel(Channel::Fast, "0.1.0-g000000000002", "github-actions").await.unwrap();
    let now: Vec<_> =
        store.channel_versions().await.unwrap().into_iter().map(|c| (c.channel, c.chart_version)).collect();
    assert_eq!(
        now,
        [(Channel::Stable, "0.1.0-g000000000001".to_string()), (Channel::Fast, "0.1.0-g000000000002".to_string())]
    );
    assert!(matches!(
        store.set_channel(Channel::Pinned, "0.1.0-g000000000003", "bob").await,
        Err(SetChannelError::Pinned)
    ));
    assert!(matches!(store.set_channel(Channel::Stable, "latest", "bob").await, Err(SetChannelError::Version(_))));
}
