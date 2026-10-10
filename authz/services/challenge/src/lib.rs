//! awzy-challenge: challenges and containments.
//!
//! The PDP calls [`Resolve`](grpc) on an enforced challenge verdict: it learns which of the
//! required kinds this exact request has passed, and gets the pending challenge for the rest.
//! The person answers it (a justification, or someone else's approval), and the retried request
//! is allowed while the grant lasts. Containments revoke an accessor's access until lifted; every
//! PDP follows the active list over `WatchContainments`.

pub mod approvals;
mod feed;
mod grpc;
mod store;

pub use feed::{Feed, HEARTBEAT};
pub use grpc::router;
pub use store::{
    ActiveContainments, Challenge, ChallengeStatus, ChallengeStore, Containment, Lifetimes, RequestKey, Resolution,
    StoreError, MIN_JUSTIFICATION_CHARS,
};
