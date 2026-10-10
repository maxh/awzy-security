//! Classifies resources in the background as connectors write them (demo plan step 4,
//! ADR 0047), with awzy-classify's rules. Each resource is looked at once per version: a write
//! sets `updated_at`, which puts it back in the queue.
//!
//! What the rules can't place keeps no tier and stays `unclassified` for policies, until the
//! model step exists.

use std::time::Duration;

use awzy_classify::{classify, Facts, Tier};

use crate::store::ContextStore;

/// Resources per pass. A full pass is followed at once by the next.
pub const BATCH: i64 = 500;

/// Classifies until the queue is empty, then checks again every `every`, forever.
pub async fn run(store: ContextStore, every: Duration) {
    loop {
        match classify_pending(&store, BATCH).await {
            Ok(n) if n as i64 == BATCH => continue,
            Ok(n) if n > 0 => tracing::info!(resources = n, "classified"),
            Ok(_) => {}
            Err(error) => tracing::warn!(%error, "classification pass failed; retrying"),
        }
        tokio::time::sleep(every).await;
    }
}

/// One pass over up to `limit` resources that are new or changed since they were classified.
/// Returns how many it looked at.
pub async fn classify_pending(store: &ContextStore, limit: i64) -> Result<usize, sqlx::Error> {
    let pending = store.resources_to_classify(limit).await?;
    for r in &pending {
        // The resource itself comes first; its folders' names are part of what it's about.
        let chain = store.resource_with_ancestors(&r.id).await?;
        let folders: Vec<&str> = chain.iter().skip(1).map(|f| f.name.as_str()).collect();
        let facts = Facts {
            kind: &r.kind,
            name: &r.name,
            sensitivity: r.sensitivity.as_deref().and_then(Tier::parse),
            labels: &r.labels,
            folders: &folders,
            text: r.text.as_deref(),
        };
        let result = classify(&facts);
        store.set_classification(r, result.as_ref()).await?;
    }
    Ok(pending.len())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::{
        resources::{entity, ResourceInput},
        store::tests::test_store,
    };

    fn input(id: &str, kind: &str, name: &str, parent: Option<&str>, text: Option<&str>) -> ResourceInput {
        ResourceInput {
            id: id.into(),
            source: "synthetic".into(),
            kind: kind.into(),
            name: name.into(),
            parent: parent.map(str::to_string),
            owner: None,
            sensitivity: None,
            labels: vec![],
            sharing: vec![],
            text: text.map(str::to_string),
            url: None,
            attributes: Value::Null,
        }
    }

    #[tokio::test]
    async fn new_and_changed_resources_are_classified_once_per_version() {
        let Some(store) = test_store().await else { return };
        store
            .put_resources(&[
                input("synthetic:folder:payroll", "folder", "Payroll", None, None),
                input("synthetic:doc:march", "document", "March", Some("synthetic:folder:payroll"), None),
                input("synthetic:doc:memo", "document", "Memo", None, Some("Strictly confidential.")),
                input("synthetic:doc:misc", "document", "Misc", None, None),
            ])
            .await
            .unwrap();
        assert_eq!(classify_pending(&store, BATCH).await.unwrap(), 4);
        assert_eq!(classify_pending(&store, BATCH).await.unwrap(), 0, "each version is classified once");

        let get = |id: &'static str| {
            let store = store.clone();
            async move { store.resource(id).await.unwrap().unwrap() }
        };
        let march = get("synthetic:doc:march").await;
        let c = march.classification.clone().unwrap();
        assert_eq!(
            (c.sensitivity.as_deref(), c.by.as_deref()),
            (Some("confidential"), Some("metadata")),
            "{}",
            c.reason
        );
        assert_eq!(entity(&march)["attrs"]["unclassified"], false);
        assert_eq!(entity(&get("synthetic:doc:memo").await)["attrs"]["sensitivity"], "crown_jewels");

        let misc = get("synthetic:doc:misc").await;
        assert_eq!(misc.classification.as_ref().unwrap().sensitivity, None, "nothing decides it");
        let attrs = &entity(&misc)["attrs"];
        assert_eq!(
            (&attrs["sensitivity"], &attrs["unclassified"]),
            (&serde_json::json!("internal"), &serde_json::json!(true))
        );
        let folder = get("synthetic:folder:payroll").await;
        assert_eq!(folder.classification.unwrap().sensitivity, None, "folders only by label");

        // A connector rewrites the memo: it's queued again and the new text decides.
        store
            .put_resources(&[input(
                "synthetic:doc:memo",
                "document",
                "Memo",
                None,
                Some("Approved for external distribution."),
            )])
            .await
            .unwrap();
        assert_eq!(classify_pending(&store, BATCH).await.unwrap(), 1);
        assert_eq!(entity(&get("synthetic:doc:memo").await)["attrs"]["sensitivity"], "public");
    }
}
