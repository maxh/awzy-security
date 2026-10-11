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
    let entities = json!({ "entities": chain_entities(&[r]), "assignees": ["bob"] });
    let read: awzy_context_api::model::ResourceEntities = serde_json::from_value(entities).unwrap();
    assert_eq!(
        (read.entities[0].uid.id.as_str(), read.assignees.as_slice()),
        ("gdrive:1", ["bob".to_string()].as_slice())
    );
}
