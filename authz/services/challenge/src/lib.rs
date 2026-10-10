//! awzy-challenge: challenge and containment state machines (justification, WebAuthn, approval, biometric).

use axum::Router;

/// Routes this service serves in addition to the shared health routes.
pub fn router() -> Router {
    Router::new()
}
