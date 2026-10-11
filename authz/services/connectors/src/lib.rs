//! awzy-connectors: syncs systems of record (IdP, HR, work tracking, data catalogs) into awzy-context.
//!
//! Each source implements [`Connector`]. Google Drive (ADR 0069): shared drives' files, folders,
//! and sharing. Jira (ADR 0070): issues as work items, linked to the Drive files they cite. Both
//! read-only.

pub mod context;
pub mod drive;
pub mod google;
pub mod http;
pub mod jira;
pub mod status;
pub mod supervisor;

use axum::Router;

/// One system of record that awzy reads entities from.
pub trait Connector: Send + Sync {
    /// Stable identifier, e.g. `okta`, `workday`, `jira`.
    fn name(&self) -> &'static str;
}

/// Webhook receivers for sources that push changes, and each connector's status.
pub fn router() -> Router {
    status::router()
}
