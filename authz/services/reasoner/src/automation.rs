//! How much a person's requests look like software working with their credentials (ADR 0085): a
//! coding agent or assistant signed in as them. The PDP sees such a request as the person, so
//! policies can ask for a security key when this score is high.
//!
//! Two signals from the last 24 hours of a person's decisions and events, each with a limit:
//!
//! - **Through MCP:** the share of their requests that came through awzy-gateway or the client
//!   portal's MCP endpoint (`client = "mcp"`), which only software calls.
//! - **Bursts:** requests in their busiest minute beyond what a person clicks through.
//!
//! Only people get one: a service account is software already.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::{activity::Principal, score::Score};

/// Below this many requests in a day there's too little to tell.
const MIN_REQUESTS: u64 = 10;
/// Points for a day entirely through MCP, scaled by the share that was.
const MOST_FOR_MCP: f64 = 60.0;
/// Requests in one minute a person rarely passes, points per request beyond it, and the most.
const HUMAN_PER_MINUTE: u64 = 20;
const POINTS_PER_EXTRA_REQUEST: u64 = 2;
const MOST_FOR_BURSTS: u64 = 40;

/// One person's last day, as far as automation goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Day {
    pub requests: u64,
    pub through_mcp: u64,
    pub busiest_minute: u64,
}

/// The score for one day, or None when there's too little to tell.
pub fn score(day: Day) -> Option<Score> {
    if day.requests < MIN_REQUESTS {
        return None;
    }
    let mut points = 0;
    let mut reasons = Vec::new();
    if day.through_mcp > 0 {
        let p = (day.through_mcp as f64 / day.requests as f64 * MOST_FOR_MCP).round() as u64;
        points += p;
        reasons.push(format!("{} of {} requests in the last day came through MCP", day.through_mcp, day.requests));
    }
    if day.busiest_minute > HUMAN_PER_MINUTE {
        points += ((day.busiest_minute - HUMAN_PER_MINUTE) * POINTS_PER_EXTRA_REQUEST).min(MOST_FOR_BURSTS);
        reasons.push(format!(
            "{} requests in its busiest minute; a person rarely passes {HUMAN_PER_MINUTE}",
            day.busiest_minute
        ));
    }
    Some(Score { score: points.min(100) as i32, reasons })
}

#[derive(Debug, clickhouse::Row, serde::Deserialize)]
struct Row {
    principal_id: String,
    requests: u64,
    through_mcp: u64,
    busiest_minute: u64,
}

const QUERY: &str = "
SELECT principal_id, sum(n) AS requests, sum(mcp) AS through_mcp, max(n) AS busiest_minute
FROM
(
    SELECT principal_id, toStartOfMinute(at) AS minute, count() AS n, countIf(client = 'mcp') AS mcp
    FROM
    (
        SELECT at, principal_id, JSONExtractString(context, 'client') AS client
        FROM decisions
        WHERE principal_type = 'Human' AND at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
        UNION ALL
        SELECT at, principal_id, JSONExtractString(attributes, 'client') AS client
        FROM events
        WHERE principal_type = 'Human' AND at > fromUnixTimestamp64Milli(?, 'UTC') AND at <= fromUnixTimestamp64Milli(?, 'UTC')
    )
    GROUP BY principal_id, minute
)
GROUP BY principal_id";

/// Each person's last day before `as_of`.
pub async fn load(clickhouse: &clickhouse::Client, as_of: DateTime<Utc>) -> anyhow::Result<BTreeMap<Principal, Day>> {
    let end = as_of.timestamp_millis();
    let start = (as_of - chrono::Duration::days(1)).timestamp_millis();
    let rows: Vec<Row> = clickhouse.query(QUERY).bind(start).bind(end).bind(start).bind(end).fetch_all().await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let day = Day { requests: r.requests, through_mcp: r.through_mcp, busiest_minute: r.busiest_minute };
            ((awzy_model::entity_type::HUMAN.to_string(), r.principal_id), day)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(requests: u64, through_mcp: u64, busiest_minute: u64) -> Day {
        Day { requests, through_mcp, busiest_minute }
    }

    #[test]
    fn a_person_clicking_through_their_day_scores_nothing() {
        assert_eq!(score(day(120, 0, 6)).unwrap().score, 0);
        assert_eq!(score(day(4, 4, 4)), None, "too little to tell");
    }

    #[test]
    fn a_day_through_mcp_in_bursts_scores_as_software() {
        let agent = score(day(400, 400, 45)).unwrap();
        assert_eq!(agent.score, 100, "60 for MCP, 40 for the burst");
        assert_eq!(agent.reasons.len(), 2);
        assert!(agent.reasons[0].contains("400 of 400 requests"), "{:?}", agent.reasons);
        let half = score(day(100, 50, 10)).unwrap();
        assert_eq!(half.score, 30, "half through MCP, no burst");
        let bursty = score(day(200, 0, 30)).unwrap();
        assert_eq!(bursty.score, 20, "ten requests past what a person clicks, two points each");
    }
}
