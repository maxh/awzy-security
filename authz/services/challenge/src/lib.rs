//! awzy-challenge: challenges and containments.
//!
//! The PDP calls [`Resolve`](grpc) on an enforced challenge verdict: it learns which of the
//! required kinds this exact request has passed, and gets the pending challenge for the rest.
//! The person answers it (a justification, or someone else's approval), and the retried request
//! is allowed while the grant lasts. Containments revoke a principal's access until lifted; every
//! PDP follows the active list over `WatchContainments`. People are told what's waiting for them
//! in the client's Slack and by email ([`notify`], ADR 0073, 0122).

pub mod approvals;
pub mod email;
mod feed;
mod grpc;
pub mod keys;
pub mod notify;
pub mod slack;
mod store;
pub mod support;
pub mod webauthn;

pub use feed::{Feed, HEARTBEAT};
pub use grpc::{router, Access};
pub use store::{
    ActiveContainments, Challenge, ChallengeStatus, ChallengeStore, ChallengedPolicy, Containment, Lifetimes,
    RequestKey, Resolution, StoreError, MIN_JUSTIFICATION_CHARS,
};
