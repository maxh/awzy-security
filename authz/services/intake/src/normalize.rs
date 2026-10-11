//! One normalizer per source format, each turning what the source sends into
//! [`EventRecord`]s. A batch is all or nothing: one bad event rejects the request, naming it, so
//! a log shipper never half-delivers and then retries the half that landed.

use awzy_model::{EntityRef, EventId, EventRecord};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Map, Value};

/// Most events one request may carry.
pub const MAX_EVENTS: usize = 1_000;

/// How far ahead of awzy's clock an event's time may be before it's refused as a bad clock.
const MAX_CLOCK_SKEW: Duration = Duration::minutes(5);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum NormalizeError {
    #[error("the request has no events")]
    Empty,
    #[error("the request has {0} events; send at most {MAX_EVENTS} at a time")]
    TooMany(usize),
    #[error("event {index}: {reason}")]
    Invalid { index: usize, reason: String },
}

fn invalid(index: usize, reason: impl Into<String>) -> NormalizeError {
    NormalizeError::Invalid { index, reason: reason.into() }
}

fn check_count(n: usize) -> Result<(), NormalizeError> {
    match n {
        0 => Err(NormalizeError::Empty),
        n if n > MAX_EVENTS => Err(NormalizeError::TooMany(n)),
        _ => Ok(()),
    }
}

fn check_time(index: usize, at: DateTime<Utc>, now: DateTime<Utc>) -> Result<(), NormalizeError> {
    if at > now + MAX_CLOCK_SKEW {
        return Err(invalid(index, format!("`at` ({at}) is more than 5 minutes in the future")));
    }
    Ok(())
}

// ---- app: awzy's own event schema, for apps that log access themselves

/// `POST /v1/events` body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppBatch {
    pub events: Vec<AppEvent>,
}

/// One event in awzy's own schema. Times are RFC 3339 and may carry any offset; they're stored
/// in UTC.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppEvent {
    pub at: DateTime<Utc>,
    /// The app that logged it.
    pub system: String,
    /// Who did it.
    pub principal: EntityRef,
    #[serde(default)]
    pub on_behalf_of: Option<EntityRef>,
    pub action: String,
    #[serde(default)]
    pub resource: Option<EntityRef>,
    /// success | failure; omitted when the app doesn't know.
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub source_ip: Option<String>,
    #[serde(default)]
    pub attributes: Map<String, Value>,
}

pub fn app(batch: AppBatch, now: DateTime<Utc>) -> Result<Vec<EventRecord>, NormalizeError> {
    check_count(batch.events.len())?;
    batch
        .events
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            for (field, value) in [
                ("system", &e.system),
                ("principal.type", &e.principal.kind),
                ("principal.id", &e.principal.id),
                ("action", &e.action),
            ] {
                if value.trim().is_empty() {
                    return Err(invalid(i, format!("`{field}` is required")));
                }
            }
            let outcome = e.outcome.unwrap_or_default();
            if !matches!(outcome.as_str(), "" | "success" | "failure") {
                return Err(invalid(i, format!("`outcome` must be success or failure, not {outcome:?}")));
            }
            check_time(i, e.at, now)?;
            let on_behalf_of = e.on_behalf_of.unwrap_or_else(|| EntityRef::new("", ""));
            let resource = e.resource.unwrap_or_else(|| EntityRef::new("", ""));
            Ok(EventRecord {
                id: EventId::new(),
                at: e.at,
                received_at: now,
                source: "app".into(),
                system: e.system,
                principal_type: e.principal.kind,
                principal_id: e.principal.id,
                on_behalf_of_type: on_behalf_of.kind,
                on_behalf_of_id: on_behalf_of.id,
                action: e.action,
                resource_type: resource.kind,
                resource_id: resource.id,
                outcome,
                source_ip: e.source_ip.unwrap_or_default(),
                attributes: Value::Object(e.attributes).to_string(),
            })
        })
        .collect()
}

// ---- aws_cloudtrail: CloudTrail's log-file format, `{"Records": [...]}`

/// `POST /v1/events/aws-cloudtrail` body: a CloudTrail log file's JSON.
#[derive(Debug, Deserialize)]
pub struct CloudTrailBatch {
    #[serde(rename = "Records")]
    pub records: Vec<Value>,
}

/// Which CloudTrail callers are people (ADR 0086): a session of an IAM Identity Center role
/// (`AWSReservedSSO_…`), or of a role the install names, whose session name is the person's user
/// name as the IdP federates it.
#[derive(Debug, Clone, Default)]
pub struct CloudTrailHumans {
    /// Roles besides Identity Center's whose sessions are people, by name.
    pub roles: Vec<String>,
}

impl CloudTrailHumans {
    /// AWZY_INTAKE_HUMAN_ROLES: role names, comma-separated.
    pub fn from_env() -> Self {
        let roles = std::env::var("AWZY_INTAKE_HUMAN_ROLES").unwrap_or_default();
        Self { roles: roles.split(',').map(|r| r.trim().to_string()).filter(|r| !r.is_empty()).collect() }
    }

    /// The person behind `arn`, an assumed-role session ARN
    /// (`arn:<partition>:sts::<account>:assumed-role/<role>/<session>`), when its role is a people role.
    pub fn human(&self, arn: &str) -> Option<String> {
        let parts: Vec<&str> = arn.splitn(6, ':').collect();
        let [_, _, "sts", _, _, resource] = parts.as_slice() else { return None };
        let [kind, role, session] = resource.splitn(3, '/').collect::<Vec<_>>()[..] else { return None };
        let human_role = role.starts_with("AWSReservedSSO_") || self.roles.iter().any(|r| r == role);
        (kind == "assumed-role" && human_role && !session.is_empty()).then(|| session.to_string())
    }
}

/// The principal is the calling AWS principal's ARN (`AwsPrincipal`), the AWS service that called
/// (`AwsService`), or, for a people role's session, the person (`Human`), with the ARN kept as
/// `awsPrincipal`.
pub fn aws_cloudtrail(
    batch: CloudTrailBatch,
    now: DateTime<Utc>,
    humans: &CloudTrailHumans,
) -> Result<Vec<EventRecord>, NormalizeError> {
    check_count(batch.records.len())?;
    batch.records.iter().enumerate().map(|(i, r)| cloudtrail_record(i, r, now, humans)).collect()
}

fn cloudtrail_record(
    i: usize,
    r: &Value,
    now: DateTime<Utc>,
    humans: &CloudTrailHumans,
) -> Result<EventRecord, NormalizeError> {
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    let required = |field: &str| match r.get(field).and_then(Value::as_str) {
        Some(s) if !s.is_empty() => Ok(s.to_string()),
        _ => Err(invalid(i, format!("`{field}` is required"))),
    };
    let at: DateTime<Utc> =
        required("eventTime")?.parse().map_err(|e| invalid(i, format!("`eventTime` isn't an RFC 3339 time: {e}")))?;
    check_time(i, at, now)?;
    let system = required("eventSource")?;
    let action = required("eventName")?;

    let identity = &r["userIdentity"];
    let (principal_type, principal_id) = match (identity["arn"].as_str(), identity["invokedBy"].as_str()) {
        (Some(arn), _) if !arn.is_empty() => ("AwsPrincipal", arn.to_string()),
        (_, Some(service)) if !service.is_empty() => ("AwsService", service.to_string()),
        _ => match identity["principalId"].as_str() {
            Some(id) if !id.is_empty() => ("AwsPrincipal", id.to_string()),
            _ => return Err(invalid(i, "`userIdentity` has no arn, invokedBy, or principalId")),
        },
    };

    // CloudTrail lists the resources for many, not all, events; the first is the one acted on.
    let resource = r["resources"].as_array().and_then(|rs| rs.first());
    let (resource_type, resource_id) = resource.map(|res| (text(&res["type"]), text(&res["ARN"]))).unwrap_or_default();

    let error_code = text(&r["errorCode"]);
    let mut attributes = Map::new();
    for key in ["eventID", "awsRegion", "recipientAccountId", "userAgent", "readOnly", "errorCode", "errorMessage"] {
        if let Some(v) = r.get(key).filter(|v| !v.is_null()) {
            attributes.insert(key.to_string(), v.clone());
        }
    }
    if let Some(t) = identity["type"].as_str() {
        attributes.insert("userIdentityType".into(), json!(t));
    }
    let (principal_type, principal_id) = match humans.human(&principal_id).filter(|_| principal_type == "AwsPrincipal")
    {
        Some(human) => {
            attributes.insert("awsPrincipal".into(), json!(principal_id));
            (awzy_model::entity_type::HUMAN, human)
        }
        None => (principal_type, principal_id),
    };

    Ok(EventRecord {
        id: EventId::new(),
        at,
        received_at: now,
        source: "aws_cloudtrail".into(),
        system,
        principal_type: principal_type.into(),
        principal_id,
        on_behalf_of_type: String::new(),
        on_behalf_of_id: String::new(),
        action,
        resource_type,
        resource_id,
        outcome: if error_code.is_empty() { "success" } else { "failure" }.into(),
        source_ip: text(&r["sourceIPAddress"]),
        attributes: Value::Object(attributes).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-09T15:00:00Z".parse().unwrap()
    }

    fn app_batch(events: Value) -> AppBatch {
        serde_json::from_value(json!({ "events": events })).unwrap()
    }

    #[test]
    fn app_events_are_stored_in_utc_with_their_fields() {
        let batch = app_batch(json!([{
            "at": "2026-10-09T10:30:00-04:00",
            "system": "jira",
            "principal": { "type": "ServiceAccount", "id": "code-helper" },
            "on_behalf_of": { "type": "Human", "id": "bob" },
            "action": "read",
            "resource": { "type": "Ticket", "id": "ENG-42" },
            "outcome": "success",
            "attributes": { "project": "ENG" }
        }]));
        let [e] = app(batch, now()).unwrap().try_into().unwrap();
        assert_eq!(e.at, "2026-10-09T14:30:00Z".parse::<DateTime<Utc>>().unwrap());
        assert_eq!(e.received_at, now());
        assert_eq!((e.source.as_str(), e.system.as_str()), ("app", "jira"));
        assert_eq!((e.principal_type.as_str(), e.principal_id.as_str()), ("ServiceAccount", "code-helper"));
        assert_eq!((e.on_behalf_of_type.as_str(), e.on_behalf_of_id.as_str()), ("Human", "bob"));
        assert_eq!((e.resource_type.as_str(), e.resource_id.as_str()), ("Ticket", "ENG-42"));
        assert_eq!(e.attributes, r#"{"project":"ENG"}"#);
        assert!(e.id.to_string().starts_with("event_"));
    }

    #[test]
    fn one_bad_app_event_rejects_the_batch_and_names_it() {
        let good = json!({ "at": "2026-10-09T14:00:00Z", "system": "jira", "principal": { "type": "Human", "id": "bob" }, "action": "read" });
        let mut no_action = good.clone();
        no_action["action"] = json!("");
        assert_eq!(app(app_batch(json!([good, no_action])), now()), Err(invalid(1, "`action` is required")));
    }

    #[test]
    fn events_from_a_clock_far_ahead_are_refused() {
        let ahead = json!([{ "at": "2026-10-09T15:10:00Z", "system": "jira", "principal": { "type": "Human", "id": "bob" }, "action": "read" }]);
        assert!(matches!(app(app_batch(ahead), now()), Err(NormalizeError::Invalid { index: 0, .. })));
    }

    #[test]
    fn unknown_outcomes_and_fields_are_refused() {
        let odd = json!([{ "at": "2026-10-09T14:00:00Z", "system": "jira", "principal": { "type": "Human", "id": "bob" }, "action": "read", "outcome": "ok" }]);
        assert!(app(app_batch(odd), now()).is_err());
        let extra = json!({ "events": [], "user": "bob" });
        assert!(serde_json::from_value::<AppBatch>(extra).is_err());
    }

    #[test]
    fn empty_and_oversized_batches_are_refused() {
        assert_eq!(app(app_batch(json!([])), now()), Err(NormalizeError::Empty));
        let e = json!({ "at": "2026-10-09T14:00:00Z", "system": "s", "principal": { "type": "Human", "id": "b" }, "action": "a" });
        let many = Value::Array(vec![e; MAX_EVENTS + 1]);
        assert_eq!(app(app_batch(many), now()), Err(NormalizeError::TooMany(MAX_EVENTS + 1)));
    }

    #[test]
    fn cloudtrail_records_become_events_from_the_calling_principal() {
        let batch: CloudTrailBatch = serde_json::from_value(json!({ "Records": [{
            "eventVersion": "1.09",
            "eventTime": "2026-10-09T14:59:58Z",
            "eventSource": "s3.amazonaws.com",
            "eventName": "GetObject",
            "awsRegion": "us-east-2",
            "sourceIPAddress": "203.0.113.7",
            "userAgent": "aws-cli/2",
            "userIdentity": { "type": "AssumedRole", "principalId": "AROA:carol", "arn": "arn:aws:sts::111122223333:assumed-role/Analyst/carol" },
            "resources": [{ "type": "AWS::S3::Object", "ARN": "arn:aws:s3:::larkspur-finance/q3-plan.xlsx" }],
            "eventID": "6f1c0d0e-0000-4000-8000-000000000001",
            "readOnly": true,
            "recipientAccountId": "111122223333"
        }, {
            "eventTime": "2026-10-09T14:59:59Z",
            "eventSource": "kms.amazonaws.com",
            "eventName": "Decrypt",
            "userIdentity": { "type": "AWSService", "invokedBy": "s3.amazonaws.com" },
            "errorCode": "AccessDenied"
        }]})).unwrap();
        let [s3, kms] = aws_cloudtrail(batch, now(), &CloudTrailHumans::default()).unwrap().try_into().unwrap();
        assert_eq!(
            (s3.source.as_str(), s3.system.as_str(), s3.action.as_str()),
            ("aws_cloudtrail", "s3.amazonaws.com", "GetObject")
        );
        assert_eq!(
            (s3.principal_type.as_str(), s3.principal_id.as_str()),
            ("AwsPrincipal", "arn:aws:sts::111122223333:assumed-role/Analyst/carol")
        );
        assert_eq!(
            (s3.resource_type.as_str(), s3.resource_id.as_str()),
            ("AWS::S3::Object", "arn:aws:s3:::larkspur-finance/q3-plan.xlsx")
        );
        assert_eq!((s3.outcome.as_str(), s3.source_ip.as_str()), ("success", "203.0.113.7"));
        let attrs: Value = serde_json::from_str(&s3.attributes).unwrap();
        assert_eq!(attrs["eventID"], "6f1c0d0e-0000-4000-8000-000000000001");
        assert_eq!(attrs["userIdentityType"], "AssumedRole");

        assert_eq!((kms.principal_type.as_str(), kms.principal_id.as_str()), ("AwsService", "s3.amazonaws.com"));
        assert_eq!(kms.outcome, "failure");
        assert_eq!(kms.resource_id, "");
    }

    #[test]
    fn cloudtrail_records_without_a_caller_are_refused() {
        let batch: CloudTrailBatch = serde_json::from_value(json!({ "Records": [{
            "eventTime": "2026-10-09T14:00:00Z", "eventSource": "s3.amazonaws.com", "eventName": "GetObject", "userIdentity": {}
        }]})).unwrap();
        assert!(matches!(
            aws_cloudtrail(batch, now(), &CloudTrailHumans::default()),
            Err(NormalizeError::Invalid { index: 0, .. })
        ));
    }

    #[test]
    fn identity_center_and_named_roles_sessions_are_people() {
        let humans = CloudTrailHumans { roles: vec!["Analyst".into()] };
        let sso = "arn:aws:sts::111122223333:assumed-role/AWSReservedSSO_ReadOnly_0123abcd/carol@larkspur.example";
        assert_eq!(humans.human(sso).as_deref(), Some("carol@larkspur.example"));
        let named = "arn:aws-us-gov:sts::111122223333:assumed-role/Analyst/dan@larkspur.example";
        assert_eq!(humans.human(named).as_deref(), Some("dan@larkspur.example"));
        for not_a_human in [
            "arn:aws:sts::111122223333:assumed-role/data-pipeline/i-0abc",
            "arn:aws:iam::111122223333:user/ci",
            "arn:aws:sts::111122223333:federated-user/AWSReservedSSO_x",
        ] {
            assert_eq!(humans.human(not_a_human), None, "{not_a_human}");
        }

        let batch: CloudTrailBatch = serde_json::from_value(json!({ "Records": [{
            "eventTime": "2026-10-09T14:00:00Z", "eventSource": "s3.amazonaws.com", "eventName": "GetObject",
            "userIdentity": { "type": "AssumedRole", "arn": sso },
        }] }))
        .unwrap();
        let [e] = aws_cloudtrail(batch, now(), &humans).unwrap().try_into().unwrap();
        assert_eq!((e.principal_type.as_str(), e.principal_id.as_str()), ("Human", "carol@larkspur.example"));
        assert!(e.attributes.contains(&format!("\"awsPrincipal\":\"{sso}\"")), "{}", e.attributes);
    }
}
