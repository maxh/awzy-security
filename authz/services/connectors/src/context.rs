//! Writing resources and work items to awzy-context (ADR 0044) through its one client (#222),
//! with this service's token (ADR 0055).

use std::collections::BTreeSet;

use awzy_context_api::{ContextApi, ContextError};
use serde_json::Value;

/// The most resources one batch carries: awzy-context's limit.
pub const BATCH: usize = 1000;

/// Who the writes are recorded as (ADR 0060).
const ACTOR: &str = "AwzyService::awzy-connectors";

#[derive(Clone)]
pub struct ContextClient {
    api: ContextApi,
}

impl ContextClient {
    pub fn new(base_url: &str, tokens: awzy_service_auth::TokenSource) -> Self {
        Self { api: ContextApi::new(base_url, tokens) }
    }

    /// Writes `resources`, replacing any with the same IDs, in batches.
    pub async fn put_resources(&self, resources: &[Value]) -> Result<(), ContextError> {
        self.api.put_resources(resources, ACTOR, BATCH).await
    }

    /// Deletes one resource. Already gone counts as done.
    pub async fn delete_resource(&self, id: &str) -> Result<(), ContextError> {
        self.api.delete_resource(id, ACTOR).await
    }

    /// Writes `items`, replacing any with the same IDs, in batches.
    pub async fn put_work_items(&self, items: &[Value]) -> Result<(), ContextError> {
        self.api.put_work_items(items, ACTOR, BATCH).await
    }

    /// Deletes one work item. Already gone counts as done.
    pub async fn delete_work_item(&self, id: &str) -> Result<(), ContextError> {
        self.api.delete_work_item(id, ACTOR).await
    }

    /// The IDs of every resource from `source`.
    pub async fn resource_ids(&self, source: &str) -> Result<BTreeSet<String>, ContextError> {
        self.api.resource_ids(source).await
    }

    /// The IDs of every work item from `source`.
    pub async fn work_item_ids(&self, source: &str) -> Result<BTreeSet<String>, ContextError> {
        self.api.work_item_ids(source).await
    }
}
