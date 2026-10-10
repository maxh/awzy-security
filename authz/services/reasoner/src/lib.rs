//! awzy-reasoner: slow path. Activity windows and peer baselines from ClickHouse, investigations, risk attributes.

use axum::Router;

/// Routes this service serves in addition to the shared health routes.
pub fn router() -> Router {
    Router::new()
}
