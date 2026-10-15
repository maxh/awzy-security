use axum::{body::Body, http::Request};
use tower::ServiceExt;

use super::*;
use crate::store::tests::test_store;

fn doc(id: &str) -> ResourceInput {
    ResourceInput {
        id: id.into(),
        source: "synthetic".into(),
        kind: "document".into(),
        name: "Q3 sales report".into(),
        parent: Some("synthetic:folder:sales".into()),
        owner: Some("nora.patel@larkspur.example".into()),
        sensitivity: Some("confidential".into()),
        labels: vec!["sales".into()],
        sharing: vec![Share { kind: "group".into(), id: "sales".into(), role: "reader".into() }],
        text: Some("Pipeline by account...".into()),
        url: None,
        attributes: Value::Null,
    }
}

#[test]
fn validation_rejects_bad_input() {
    assert!(doc("synthetic:doc:1").validate().is_ok());
    let bad = |f: &dyn Fn(&mut ResourceInput)| {
        let mut r = doc("synthetic:doc:1");
        f(&mut r);
        r.validate().is_err()
    };
    assert!(bad(&|r| r.id = "has space".into()));
    assert!(bad(&|r| r.kind = "Document".into()));
    assert!(bad(&|r| r.sensitivity = Some("secret".into())));
    assert!(bad(&|r| r.parent = Some(r.id.clone())));
    assert!(bad(&|r| r.sharing[0].kind = "everyone".into()));
    assert!(bad(&|r| r.sharing[0].id = String::new()));
    assert!(bad(&|r| r.text = Some("x".repeat(MAX_TEXT + 1))));
    assert!(bad(&|r| r.attributes = json!([1])));
    let work = WorkItemInput {
        id: "synthetic:ticket:7".into(),
        source: "synthetic".into(),
        kind: "ticket".into(),
        title: "Fix login".into(),
        state: "done".into(),
        assignees: vec![],
        resources: vec![],
        attributes: Value::Null,
    };
    assert!(work.validate().is_err(), "state is open or closed");
}

#[test]
fn unclassified_resources_get_the_default_tier() {
    let now = Utc::now();
    let r = Resource {
        id: "synthetic:doc:1".into(),
        source: "synthetic".into(),
        kind: "document".into(),
        name: "Notes".into(),
        parent: None,
        owner: None,
        sensitivity: None,
        labels: vec![],
        sharing: vec![],
        text: None,
        url: None,
        attributes: json!({}),
        created_at: now,
        updated_at: now,
        classification: None,
    };
    assert_eq!(entity(&r)["attrs"]["sensitivity"], DEFAULT_SENSITIVITY);
    assert_eq!(entity(&r)["parents"], json!([]));

    // The higher of the label and the classifier.
    let classified = |tier: &str| Classification {
        sensitivity: Some(tier.into()),
        by: Some("content".into()),
        reason: String::new(),
        at: now,
    };
    let r = Resource { classification: Some(classified("crown_jewels")), ..r };
    assert_eq!(
        (&entity(&r)["attrs"]["sensitivity"], &entity(&r)["attrs"]["unclassified"]),
        (&json!("crown_jewels"), &json!(false))
    );
    let r = Resource { sensitivity: Some("confidential".into()), classification: Some(classified("public")), ..r };
    assert_eq!(entity(&r)["attrs"]["sensitivity"], "confidential", "a classifier never lowers a label");
}

#[test]
fn sharing_comes_from_the_resource_and_its_folders() {
    let now = Utc::now();
    let share = |kind: &str, id: &str, role: &str| Share { kind: kind.into(), id: id.into(), role: role.into() };
    let resource = |id: &str, parent: Option<&str>, sharing: Vec<Share>| Resource {
        id: id.into(),
        source: "synthetic".into(),
        kind: "document".into(),
        name: id.into(),
        parent: parent.map(str::to_string),
        owner: None,
        sensitivity: None,
        labels: vec![],
        sharing,
        text: None,
        url: None,
        attributes: json!({}),
        created_at: now,
        updated_at: now,
        classification: None,
    };
    let doc = resource("doc", Some("folder"), vec![share("human", "sam", "reader")]);
    let folder = resource("folder", None, vec![share("group", "engineering", "writer")]);
    let chain = chain_entities(&[doc, folder]);
    let names = |v: &Value| -> Vec<String> {
        v.as_array().unwrap().iter().map(|e| e["__entity"]["id"].as_str().unwrap().to_string()).collect()
    };
    let a = &chain[0]["attrs"];
    assert_eq!(names(&a["shared_humans"]), ["sam"]);
    assert_eq!(names(&a["shared_groups"]), ["engineering"], "inherited from its folder");
    assert_eq!(names(&a["writer_groups"]), ["engineering"], "a writer on the folder writes the document");
    assert_eq!((names(&a["writer_humans"]).len(), &a["shared_with_everyone"]), (0, &json!(false)), "sam only reads");
    assert_eq!(names(&chain[1]["attrs"]["shared_humans"]).len(), 0, "a folder doesn't inherit from its document");

    let public = resource("public", None, vec![share("anyone", "", "reader"), share("group", "x", "reader")]);
    assert_eq!(entity(&public)["attrs"]["shared_with_everyone"], true);
    assert!(entity(&public)["attrs"].get("owner").is_none(), "no owner, no attribute");
}

/// The change log from `after` once it names `want`: other tests' transactions running at the
/// same time hold its horizon back for a moment.
async fn changes_naming(store: &ContextStore, after: i64, want: usize) -> (i64, Vec<String>, bool) {
    for _ in 0..100 {
        let answer = store.resource_changes(Some(after), 100).await.unwrap();
        if answer.1.len() >= want || answer.2 {
            return answer;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    store.resource_changes(Some(after), 100).await.unwrap()
}

#[tokio::test]
async fn the_change_log_names_every_resource_whose_entity_may_have_changed() {
    let Some(store) = test_store().await else { return };
    let (start, ids, reset) = store.resource_changes(None, 100).await.unwrap();
    assert!(ids.is_empty() && !reset, "a first call only gets the cursor");

    store.put_resources(&[doc("synthetic:doc:1")]).await.unwrap();
    let (cursor, ids, _) = changes_naming(&store, start, 1).await;
    assert_eq!(ids, ["synthetic:doc:1"]);

    // Work changes the assignees of what it links, before and after.
    let work = WorkItemInput {
        id: "synthetic:ticket:1".into(),
        source: "synthetic".into(),
        kind: "ticket".into(),
        title: "Q3 close".into(),
        state: "open".into(),
        assignees: vec!["sam".into()],
        resources: vec!["synthetic:doc:2".into()],
        attributes: Value::Null,
    };
    store.put_work_items(&[work]).await.unwrap();
    assert!(store.delete_resource("synthetic:doc:1").await.unwrap());
    let (_, ids, reset) = changes_naming(&store, cursor, 2).await;
    assert_eq!((ids, reset), (vec!["synthetic:doc:1".to_string(), "synthetic:doc:2".to_string()], false));

    let (_, _, reset) = store.resource_changes(Some(i64::MAX), 100).await.unwrap();
    assert!(reset, "a cursor from another database resets");
}

#[tokio::test]
async fn a_write_that_commits_late_is_still_ahead_of_the_cursor() {
    let Some(store) = test_store().await else { return };
    let (start, _, _) = store.resource_changes(None, 100).await.unwrap();
    // A write takes its row, then waits to commit while another write commits and is read.
    let mut slow = store.pool().begin().await.unwrap();
    sqlx::query("INSERT INTO resource_changes (resource_id) VALUES ('synthetic:doc:slow')")
        .execute(&mut *slow)
        .await
        .unwrap();
    store.put_resources(&[doc("synthetic:doc:fast")]).await.unwrap();
    let (cursor, ids, _) = store.resource_changes(Some(start), 100).await.unwrap();
    assert!(!ids.contains(&"synthetic:doc:fast".to_string()), "held back by the slow write: {ids:?}");
    slow.commit().await.unwrap();

    let (_, ids, reset) = changes_naming(&store, cursor, 2).await;
    assert!(!reset);
    assert_eq!(ids, ["synthetic:doc:fast", "synthetic:doc:slow"], "neither is skipped");
}

#[tokio::test]
async fn resources_and_work_items_round_trip() {
    let Some(store) = test_store().await else { return };
    let mut folder = doc("synthetic:folder:sales");
    folder.kind = "folder".into();
    folder.parent = None;
    assert_eq!(store.put_resources(&[folder, doc("synthetic:doc:1"), doc("synthetic:doc:2")]).await.unwrap(), 3);

    let r = store.resource("synthetic:doc:1").await.unwrap().unwrap();
    assert_eq!(r.sharing[0].id, "sales");
    assert_eq!(r.attributes, json!({}));
    assert_eq!(
        entity(&r),
        json!({ "uid": { "type": "Resource", "id": "synthetic:doc:1" },
                 "attrs": { "sensitivity": "confidential", "unclassified": false,
                            "owner": { "__entity": { "type": "Human", "id": "nora.patel@larkspur.example" } },
                            "shared_humans": [], "shared_groups": [{ "__entity": { "type": "Group", "id": "sales" } }],
                            "writer_humans": [], "writer_groups": [], "shared_with_everyone": false },
                 "parents": [{ "type": "Resource", "id": "synthetic:folder:sales" }] })
    );

    // Writing again replaces.
    let mut changed = doc("synthetic:doc:1");
    changed.sensitivity = Some("crown_jewels".into());
    store.put_resources(&[changed]).await.unwrap();
    assert_eq!(store.resource("synthetic:doc:1").await.unwrap().unwrap().sensitivity.as_deref(), Some("crown_jewels"));

    let in_folder = ResourceFilter { parent: Some("synthetic:folder:sales".into()), ..Default::default() };
    assert_eq!(store.resources(&in_folder).await.unwrap().len(), 2);
    let page = ResourceFilter { limit: Some(1), after: Some("synthetic:doc:1".into()), ..Default::default() };
    assert_eq!(store.resources(&page).await.unwrap()[0].id, "synthetic:doc:2");
    // Search matches the ID or name, ignoring case, and takes % and _ literally.
    let search = |q: &str| ResourceFilter { q: Some(q.into()), ..Default::default() };
    let found: Vec<String> = store.resources(&search("DOC:2")).await.unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(found, ["synthetic:doc:2"]);
    assert!(store.resources(&search("%")).await.unwrap().is_empty());

    // A batch with one bad item writes nothing.
    let mut bad = doc("synthetic:doc:3");
    bad.sensitivity = Some("nope".into());
    assert!(store.put_resources(&[doc("synthetic:doc:4"), bad]).await.is_err());
    assert!(store.resource("synthetic:doc:4").await.unwrap().is_none());

    let work = WorkItemInput {
        id: "synthetic:account:acme-bank".into(),
        source: "synthetic".into(),
        kind: "account".into(),
        title: "Acme Bank (Northeast financial services)".into(),
        state: "open".into(),
        assignees: vec!["nora.patel@larkspur.example".into(), "sales-genie".into()],
        resources: vec!["synthetic:doc:1".into()],
        attributes: Value::Null,
    };
    store.put_work_items(&[work]).await.unwrap();
    let by_assignee = WorkItemFilter { assignee: Some("sales-genie".into()), ..Default::default() };
    assert_eq!(store.work_items(&by_assignee).await.unwrap().len(), 1);
    let by_resource = WorkItemFilter { resource: Some("synthetic:doc:1".into()), ..Default::default() };
    assert_eq!(store.work_items(&by_resource).await.unwrap()[0].kind, "account");
    let by_source = |source: &str| WorkItemFilter { source: Some(source.into()), ..Default::default() };
    assert_eq!(store.work_items(&by_source("synthetic")).await.unwrap().len(), 1);
    assert!(store.work_items(&by_source("jira")).await.unwrap().is_empty());

    assert!(store.delete_resource("synthetic:doc:2").await.unwrap());
    assert!(!store.delete_resource("synthetic:doc:2").await.unwrap());
}

#[tokio::test]
async fn the_http_api_writes_reads_and_renders_entities() {
    let Some(store) = test_store().await else { return };
    let app = router(store);
    let send = |method: &str, path: &str, body: Option<Value>| {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(body.map(|b| Body::from(b.to_string())).unwrap_or_default())
            .unwrap();
        let app = app.clone();
        async move {
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            (status, serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null))
        }
    };

    let batch = json!({ "resources": [
        { "id": "synthetic:doc:1", "source": "synthetic", "kind": "document", "name": "Strategic plan",
          "sensitivity": "crown_jewels", "sharing": [{ "kind": "group", "id": "strategy" }] },
        { "id": "synthetic:doc:2", "source": "synthetic", "kind": "document", "name": "Lunch menu" },
    ]});
    let (status, body) = send("POST", "/v1/resources/batch", Some(batch)).await;
    assert_eq!((status, body["written"].clone()), (StatusCode::OK, json!(2)));

    let (status, body) = send("GET", "/v1/resources/synthetic:doc:1", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["resource"]["sharing"][0]["role"], "reader", "role defaults to reader");

    let (_, entity) = send("GET", "/v1/resources/synthetic:doc:2/entity", None).await;
    assert_eq!(entity["attrs"]["sensitivity"], "internal", "unclassified: the default tier");
    assert_eq!(entity["attrs"]["unclassified"], true);

    // A folder chain, with a cycle a connector wrote by mistake (a/b's parent is a/b's child).
    let tree = json!({ "resources": [
        { "id": "synthetic:folder:a", "source": "synthetic", "kind": "folder", "name": "A",
          "parent": "synthetic:folder:a/b/c", "sensitivity": "internal" },
        { "id": "synthetic:folder:a/b", "source": "synthetic", "kind": "folder", "name": "B",
          "parent": "synthetic:folder:a" },
        { "id": "synthetic:folder:a/b/c", "source": "synthetic", "kind": "document", "name": "C",
          "parent": "synthetic:folder:a/b", "sensitivity": "confidential" },
    ]});
    send("POST", "/v1/resources/batch", Some(tree)).await;
    let (status, body) = send("GET", "/v1/resources/synthetic:folder:a%2Fb%2Fc/entities", None).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> =
        body["entities"].as_array().unwrap().iter().map(|e| e["uid"]["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["synthetic:folder:a/b/c", "synthetic:folder:a/b", "synthetic:folder:a"],
        "nearest first, once each"
    );
    let attrs = &body["entities"][0]["attrs"];
    assert_eq!((&attrs["sensitivity"], &attrs["unclassified"]), (&json!("confidential"), &json!(false)));
    assert_eq!(body["entities"][0]["parents"], json!([{ "type": "Resource", "id": "synthetic:folder:a/b" }]));
    assert_eq!(body["assignees"], json!([]), "no work items yet");

    // Work on the top folder covers what's inside it; closed work doesn't count.
    let work = json!({ "work_items": [
        { "id": "synthetic:ticket:1", "source": "synthetic", "kind": "ticket", "title": "Open",
          "assignees": ["sam.okafor@larkspur.example", "code-helper"], "resources": ["synthetic:folder:a"] },
        { "id": "synthetic:ticket:2", "source": "synthetic", "kind": "ticket", "title": "Done", "state": "closed",
          "assignees": ["lee.kowalski@larkspur.example"], "resources": ["synthetic:folder:a/b/c"] },
    ]});
    send("POST", "/v1/work-items/batch", Some(work)).await;
    let (_, body) = send("GET", "/v1/resources/synthetic:folder:a%2Fb%2Fc/entities", None).await;
    assert_eq!(body["assignees"], json!(["code-helper", "sam.okafor@larkspur.example"]));
    assert_eq!(
        body["work"],
        json!([{ "id": "synthetic:ticket:1", "assignees": ["sam.okafor@larkspur.example", "code-helper"] }]),
        "each open item that links it, with who's on it (ADR 0118)"
    );

    let (status, _) = send("GET", "/v1/resources/synthetic:nothing/entities", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        "PUT",
        "/v1/resources/bad%20id",
        Some(json!({
        "source": "synthetic", "kind": "document", "name": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = send("DELETE", "/v1/resources/synthetic:doc:2", None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send("GET", "/v1/resources/synthetic:doc:2", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
