//! The contract with awzy-context's clients (#222): what this server writes reads back through
//! `awzy-context-api`'s types, field for field.

use chrono::Utc;
use serde_json::json;

use crate::resources::{chain_entities, Classification, Resource, Share};

/// A resource as this server writes it reads back through the client's type.
#[test]
fn a_resource_reads_back_through_the_client_api() {
    let at = chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00Z").unwrap().with_timezone(&Utc);
    let r = Resource {
        id: "gdrive:1".into(),
        source: "google_drive".into(),
        kind: "document".into(),
        name: "Board deck".into(),
        parent: Some("gdrive:folder".into()),
        owner: Some("alice".into()),
        sensitivity: Some("confidential".into()),
        labels: vec!["board".into()],
        sharing: vec![Share { kind: "group".into(), id: "board".into(), role: "writer".into() }],
        text: None,
        url: Some("https://docs.example/1".into()),
        attributes: json!({ "mime": "doc" }),
        created_at: at,
        updated_at: at,
        classification: Some(Classification {
            sensitivity: Some("crown_jewels".into()),
            by: Some("label".into()),
            reason: "labelled".into(),
            at,
        }),
    };
    let wire = serde_json::to_value(json!({ "resource": r })).unwrap();
    let read: awzy_context_api::model::ResourceOne = serde_json::from_value(wire).unwrap();
    let c = read.resource;
    assert_eq!(
        (c.id.as_str(), c.parent.as_deref(), c.sharing[0].role.as_str()),
        ("gdrive:1", Some("gdrive:folder"), "writer")
    );
    assert_eq!(c.classification.unwrap().sensitivity.as_deref(), Some("crown_jewels"));
    let entities = json!({ "entities": chain_entities(&[r]), "assignees": ["bob"],
        "work": [{ "id": "jira:OPS-1", "assignees": ["bob"] }] });
    let read: awzy_context_api::model::ResourceEntities = serde_json::from_value(entities).unwrap();
    assert_eq!(
        (read.entities[0].uid.id.as_str(), read.assignees.as_slice()),
        ("gdrive:1", ["bob".to_string()].as_slice())
    );
    assert_eq!((read.work[0].id.as_str(), read.work[0].assignees.as_slice()), ("jira:OPS-1", ["bob".to_string()].as_slice()));
}

/// An investigation as this server writes it reads back through the client's type.
#[test]
fn an_investigation_reads_back_through_the_client_api() {
    use crate::investigations::{Investigation, InvestigationInput};

    let at = chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00Z").unwrap().with_timezone(&Utc);
    let i = Investigation {
        qid: "investigation_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90".into(),
        input: InvestigationInput {
            principal: awzy_model::EntityRef::new("Human", "nora"),
            trigger: "risk 63".into(),
            floor: 63,
            status: "needs_review".into(),
            verdict: "suspicious".into(),
            risk: 70,
            summary: "exported the board deck".into(),
            evidence: json!([{ "id": "decision_1", "why": "the export" }]),
            containment: "none".into(),
            notes: vec!["checked the share".into()],
            finding: None,
            model: "m".into(),
            input_tokens: 10,
            output_tokens: 2,
            started_at: at,
        },
        finished_at: at,
        closed_by: Some("Human::priya".into()),
        closed_at: Some(at),
        close_reason: Some("benign".into()),
    };
    let wire = serde_json::to_value(json!({ "investigation": i })).unwrap();
    let read: awzy_context_api::model::InvestigationOne = serde_json::from_value(wire).unwrap();
    let c = read.investigation;
    assert_eq!((c.principal.kind.as_str(), c.principal.id.as_str()), ("Human", "nora"));
    assert_eq!((c.evidence[0].why.as_str(), c.close_reason.as_deref()), ("the export", Some("benign")));
}
