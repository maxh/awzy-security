//! awzy-connectors: syncs systems of record (IdP, HR, work tracking, data catalogs) into awzy-context.
//!
//! Each source implements [`Connector`]. Google Drive is the first (ADR 0069): shared drives'
//! files, folders, and sharing, read-only.

pub mod context;
pub mod drive;
pub mod google;
pub mod http;

use axum::Router;

/// One system of record that awzy reads entities from.
pub trait Connector: Send + Sync {
    /// Stable identifier, e.g. `okta`, `workday`, `jira`.
    fn name(&self) -> &'static str;
}

/// Webhook receivers for sources that push changes.
pub fn router() -> Router {
    Router::new()
}
