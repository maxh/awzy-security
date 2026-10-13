//! Decisions for a period, as a file an auditor keeps (#323).
//!
//! The export reads what the Decisions page's filters select, through the same bounded search,
//! so it can't reach further than the page can: a window of at most 30 days, anywhere in the
//! history. The request context goes in only for those who may see `Decision.context` (ADR 0103).
//! Each export is recorded in the activity log with who made it and what it covered, so an
//! export is as visible as the change it might be used to examine.

use async_graphql::{Context, Enum, Object, Result, SimpleObject};
use awzy_model::DecisionId;
use awzy_portal::{ErrorCode, JsonString, UtcTimestamp};
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};

use crate::{
    access::{field_visible, require, PortalAction, PortalArea, Surface, Viewer},
    clickhouse,
    decisions::{search_up_to, window, DecisionFilter, DecisionRow, SearchParams},
    Services,
};

/// The most decisions one export holds; past it, `truncated` says so and a narrower filter or a
/// shorter period gets the rest.
const MAX_EXPORT: usize = 50_000;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum DecisionExportFormat {
    /// One row per decision, for a spreadsheet.
    Csv,
    /// An array of objects, one per decision.
    Json,
}

/// An export, ready to save. Exactly one of `csv` and `json` is set, as the format asked.
#[derive(SimpleObject)]
pub struct DecisionExport {
    /// A file name that says what it covers.
    filename: String,
    /// The period it covers: decisions at or after `since` and before `until`.
    since: UtcTimestamp,
    until: UtcTimestamp,
    /// How many decisions it holds, newest first.
    count: i32,
    /// More decisions matched than one export holds; narrow the filter or the period.
    truncated: bool,
    /// Whether each decision's request context is included: only for those who may see it.
    includes_context: bool,
    csv: Option<String>,
    json: Option<JsonString>,
}

#[derive(Default)]
pub struct DecisionExportQuery;

#[Object]
impl DecisionExportQuery {
    /// The decisions the filter selects, newest first, as a file. Recorded in the activity log.
    async fn decision_export(
        &self,
        ctx: &Context<'_>,
        filter: Option<DecisionFilter>,
        format: DecisionExportFormat,
    ) -> Result<DecisionExport> {
        require(ctx, PortalAction::View, PortalArea::Decisions).await?;
        let services = ctx.data::<Services>()?;
        let viewer = ctx.data::<Viewer>()?;
        let filter = filter.unwrap_or_default();
        let (since, until) = window(filter.since.map(|t| t.0), filter.until.map(|t| t.0), Utc::now());
        if since >= until {
            return Err(ErrorCode::InvalidInput.error("the period ends before it starts"));
        }
        let includes_context =
            field_visible(services, viewer, Surface::Portal, PortalArea::Decisions, "Decision.context").await?;

        let page = search_up_to(
            &clickhouse(ctx)?,
            SearchParams {
                principal_id: filter.principal_id.clone(),
                resource_id: filter.resource_id.clone(),
                verdict: filter.verdict,
                mode: filter.mode.map(Into::into),
                since: Some(since),
                until: Some(until),
                after: None,
                limit: MAX_EXPORT,
            },
            MAX_EXPORT,
        )
        .await?;

        let records: Vec<Map<String, Value>> = page.rows.iter().map(|r| record(r, includes_context)).collect();
        let (csv, json, extension) = match format {
            DecisionExportFormat::Csv => (Some(to_csv(&records, includes_context)), None, "csv"),
            DecisionExportFormat::Json => (None, Some(JsonString::of(&Value::from(records.clone()))), "json"),
        };
        let filename = format!("awzy-decisions-{}-to-{}.{extension}", file_time(since), file_time(until));

        let mut activity = awzy_activity::Activity::now(
            viewer.subject.clone(),
            viewer.on_behalf_of.clone(),
            "exportDecisions",
            None,
        )
        .with_outcome(Some("success"))
        .with_attribute("client", "portal")
        .with_attribute("since", rfc3339(since))
        .with_attribute("until", rfc3339(until))
        .with_attribute("count", records.len())
        .with_attribute("truncated", page.has_next)
        .with_attribute("includes_context", includes_context);
        for (key, value) in [
            ("principal_id", filter.principal_id),
            ("resource_id", filter.resource_id),
            ("verdict", filter.verdict.map(|v| v.as_str().to_string())),
            ("mode", filter.mode.map(|m| awzy_model::EnforcementMode::from(m).as_str().to_string())),
        ] {
            if let Some(value) = value {
                activity = activity.with_attribute(key, value);
            }
        }
        services.activity.record(activity);

        Ok(DecisionExport {
            filename,
            since: UtcTimestamp(since),
            until: UtcTimestamp(until),
            count: i32::try_from(records.len()).expect("an export holds at most MAX_EXPORT decisions"),
            truncated: page.has_next,
            includes_context,
            csv,
            json,
        })
    }
}

/// The columns, in order: what an auditor needs to say who did what, when, and why it was allowed.
const COLUMNS: &[&str] = &[
    "qid",
    "at",
    "principal_type",
    "principal_id",
    "on_behalf_of_type",
    "on_behalf_of_id",
    "on_behalf_of_verdict",
    "action",
    "resource_type",
    "resource_id",
    "verdict",
    "mode",
    "policies",
    "policy_version",
    "work_item_id",
    "stale_inputs",
];

/// One decision as the export writes it.
fn record(r: &DecisionRow, includes_context: bool) -> Map<String, Value> {
    let mut m = Map::new();
    let row = json!({
        "qid": DecisionId::from_uuid(r.id).to_string(),
        "at": rfc3339(r.at),
        "principal_type": r.principal_type,
        "principal_id": r.principal_id,
        "on_behalf_of_type": r.on_behalf_of_type,
        "on_behalf_of_id": r.on_behalf_of_id,
        "on_behalf_of_verdict": r.on_behalf_of_verdict,
        "action": r.action,
        "resource_type": r.resource_type,
        "resource_id": r.resource_id,
        "verdict": r.verdict,
        "mode": r.mode,
        "policies": r.policies,
        "policy_version": r.policy_version,
        "work_item_id": r.work_item_id,
        "stale_inputs": r.stale_inputs,
    });
    if let Value::Object(fields) = row {
        m.extend(fields);
    }
    if includes_context {
        // A context that isn't JSON (a decision logged before contexts were) reads as empty.
        m.insert("context".into(), serde_json::from_str(&r.context).unwrap_or_else(|_| json!({})));
    }
    m
}

/// CSV with a header row. Lists are joined with spaces (policy IDs and stale inputs have none),
/// and the context is its JSON text.
fn to_csv(records: &[Map<String, Value>], includes_context: bool) -> String {
    let mut columns = COLUMNS.to_vec();
    if includes_context {
        columns.push("context");
    }
    let mut out = columns.join(",");
    out.push_str("\r\n");
    for record in records {
        let cells: Vec<String> = columns
            .iter()
            .map(|c| match record.get(*c) {
                Some(Value::String(s)) => cell(s),
                Some(Value::Array(items)) => {
                    cell(&items.iter().map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_string)).collect::<Vec<_>>().join(" "))
                }
                Some(Value::Null) | None => String::new(),
                Some(v) => cell(&v.to_string()),
            })
            .collect();
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    out
}

/// One CSV cell (RFC 4180). A value a spreadsheet would run as a formula gets a leading `'`,
/// since IDs come from the client's systems and anyone can name a resource `=HYPERLINK(...)`.
fn cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// A time for a file name: no colons, which some systems refuse.
fn file_time(t: DateTime<Utc>) -> String {
    t.format("%Y%m%dT%H%M%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn row(resource_id: &str, context: &str) -> DecisionRow {
        DecisionRow {
            id: Uuid::nil(),
            at: Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap(),
            principal_type: "ServiceAccount".into(),
            principal_id: "sales-genie".into(),
            on_behalf_of_type: "Human".into(),
            on_behalf_of_id: "bob".into(),
            human_present: true,
            action: "read".into(),
            resource_type: "Resource".into(),
            resource_id: resource_id.into(),
            verdict: "allow".into(),
            mode: "enforce".into(),
            challenge: String::new(),
            policies: vec!["base-read".into(), "assigned".into()],
            on_behalf_of_verdict: "allow".into(),
            on_behalf_of_policies: vec![],
            policy_version: 7,
            context: context.into(),
            resource_attributes_from: String::new(),
            work_item_id: String::new(),
            stale_inputs: vec![],
        }
    }

    #[test]
    fn a_csv_export_has_a_header_and_a_row_per_decision() {
        let csv = to_csv(&[record(&row("q3-plan", "{}"), false)], false);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], COLUMNS.join(","));
        assert_eq!(
            lines[1],
            "decision_00000000-0000-0000-0000-000000000000,2026-04-01T12:00:00.000Z,ServiceAccount,sales-genie,\
             Human,bob,allow,read,Resource,q3-plan,allow,enforce,base-read assigned,7,,"
        );
    }

    #[test]
    fn the_context_is_exported_only_to_those_who_may_see_it() {
        let r = row("q3-plan", r#"{"client":"loadtest"}"#);
        assert!(!record(&r, false).contains_key("context"));
        assert_eq!(record(&r, true)["context"], json!({ "client": "loadtest" }));
        let csv = to_csv(&[record(&r, true)], true);
        assert!(csv.starts_with(&format!("{},context\r\n", COLUMNS.join(","))));
        assert!(csv.contains(r#","{""client"":""loadtest""}""#), "{csv}");
    }

    #[test]
    fn a_cell_a_spreadsheet_would_run_is_written_as_text() {
        assert_eq!(cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(cell("-1"), "'-1");
        assert_eq!(cell("a,b"), "\"a,b\"");
        assert_eq!(cell("plain"), "plain");
    }
}
