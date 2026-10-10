//! awzy-policy: versioned policy bundles, validation, and publishing to the PDPs; and the
//! enforcement mode of each scope (ADR 0007).
//!
//! A bundle is a Cedar schema plus policies. It is validated before it is stored, so every
//! bundle a PDP can fetch is one it can load. Dry-run against past decisions and bundle
//! signing come next.

mod grpc;
mod store;

pub use grpc::router;
pub use store::{
    BreakGlassOutcome, BreakGlassRequest, BundleStatus, EnforcementSetting, PolicyBundle, PolicyStore, StoreError,
    INSTALL_SCOPE,
};
