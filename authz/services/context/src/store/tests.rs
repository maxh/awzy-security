use std::str::FromStr;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use super::*;

pub(crate) async fn test_store() -> Option<ContextStore> {
    let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
        eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
        return None;
    };
    let schema = format!("test_{}", Uuid::now_v7().simple());
    let admin = PgPool::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
    let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
    let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
    let store = ContextStore::new(pool);
    store.migrate().await.unwrap();
    Some(store)
}

fn user(name: &str) -> UserInput {
    UserInput {
        user_name: name.into(),
        external_id: None,
        display_name: None,
        active: true,
        resource: json!({ "userName": name }),
    }
}

#[tokio::test]
async fn snapshot_has_people_groups_and_nesting() {
    let Some(store) = test_store().await else { return };
    let alice = store.create_user(&user("alice@acme.com")).await.unwrap();
    let bob = store.create_user(&user("bob@acme.com")).await.unwrap();
    let appsec = store
        .create_group(&GroupInput { display_name: "appsec".into(), external_id: None }, &[alice.scim_id])
        .await
        .unwrap();
    let security = store
        .create_group(&GroupInput { display_name: "security".into(), external_id: None }, &[appsec.scim_id])
        .await
        .unwrap();
    store
        .create_group(
            &GroupInput { display_name: "everyone".into(), external_id: None },
            &[alice.scim_id, bob.scim_id, Uuid::now_v7()],
        )
        .await
        .unwrap();

    let (v1, entities) = store.snapshot().await.unwrap();
    let find = |id: &str| entities.iter().find(|e| e["uid"]["id"] == id).unwrap().clone();
    assert_eq!(
        find("alice@acme.com")["parents"],
        json!([{ "type": "Group", "id": "appsec" }, { "type": "Group", "id": "everyone" }])
    );
    assert_eq!(find("appsec")["parents"], json!([{ "type": "Group", "id": "security" }]), "nested group");
    assert_eq!(store.get_group(security.scim_id).await.unwrap().unwrap().members[0].display, "appsec");

    let mut deactivated = user("alice@acme.com");
    deactivated.active = false;
    store.replace_user(alice.scim_id, &deactivated).await.unwrap();
    let (v2, entities) = store.snapshot().await.unwrap();
    assert!(v2 > v1, "every change moves the version");
    let alice_entity = entities.iter().find(|e| e["uid"]["id"] == "alice@acme.com").unwrap();
    assert_eq!((alice_entity["attrs"]["active"].clone(), alice_entity["parents"].clone()), (json!(false), json!([])));

    // Deleted or renamed away from: still in the snapshot, inactive, so a PDP never falls back
    // to the groups in an older token. Provisioned again, they're active.
    let removed = |entities: &[Value], id: &str| {
        entities.iter().find(|e| e["uid"]["id"] == id).map(|e| (e["attrs"]["active"].clone(), e["parents"].clone()))
    };
    assert!(store.delete_user(bob.scim_id).await.unwrap());
    store.replace_user(alice.scim_id, &user("alice.new@acme.com")).await.unwrap();
    let (_, entities) = store.snapshot().await.unwrap();
    assert_eq!(removed(&entities, "bob@acme.com"), Some((json!(false), json!([]))));
    assert_eq!(removed(&entities, "alice@acme.com"), Some((json!(false), json!([]))));
    assert_eq!(removed(&entities, "alice.new@acme.com").unwrap().0, json!(true));
    store.create_user(&user("bob@acme.com")).await.unwrap();
    let (_, entities) = store.snapshot().await.unwrap();
    let bobs: Vec<_> = entities.iter().filter(|e| e["uid"]["id"] == "bob@acme.com").collect();
    assert_eq!((bobs.len(), bobs[0]["attrs"]["active"].clone()), (1, json!(true)), "back, and only once");

    assert!(
        matches!(store.create_user(&user("BOB@acme.com")).await, Err(StoreError::Conflict(_))),
        "userName is case-insensitive"
    );
}

#[tokio::test]
async fn service_accounts_need_a_provisioned_owner_and_reach_the_snapshot() {
    let Some(store) = test_store().await else { return };
    store.create_user(&user("bob@acme.com")).await.unwrap();
    let input = |owner: &str| ServiceAccountInput {
        id: "sales-genie".into(),
        owner: owner.into(),
        ai: true,
        groups: vec!["engineering".into()],
        description: "Drafts follow-up emails".into(),
    };
    assert!(matches!(
        store.put_service_account(&input("mallory@acme.com"), "alice").await,
        Err(StoreError::Invalid(_))
    ));
    let mut bad = input("bob@acme.com");
    bad.id = "has spaces".into();
    assert!(matches!(store.put_service_account(&bad, "alice").await, Err(StoreError::Invalid(_))));

    let sa = store.put_service_account(&input("BOB@acme.com"), "alice").await.unwrap();
    assert_eq!(
        (sa.owner.as_str(), sa.created_by.as_str()),
        ("bob@acme.com", "alice"),
        "the owner is stored as provisioned"
    );
    let (v1, entities) = store.snapshot().await.unwrap();
    let genie = entities.iter().find(|e| e["uid"]["type"] == "ServiceAccount").unwrap();
    assert_eq!(
        genie,
        &json!({ "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                 "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob@acme.com" } }, "ai": true },
                 "parents": [{ "type": "Group", "id": "engineering" }] })
    );

    let mut changed = input("bob@acme.com");
    changed.ai = false;
    assert!(!store.put_service_account(&changed, "carol").await.unwrap().ai);
    assert!(store.delete_service_account("sales-genie").await.unwrap());
    let (v2, entities) = store.snapshot().await.unwrap();
    assert!(v2 > v1 && !entities.iter().any(|e| e["uid"]["type"] == "ServiceAccount"));
}
