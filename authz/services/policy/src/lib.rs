//! awzy-policy: versioned policy bundles, validation, and publishing to the PDPs; the
//! enforcement mode of each scope (ADR 0007); and what enforcement points do while the PDP is
//! unreachable (ADR 0089).
//!
//! A bundle is a Cedar schema plus policies. It is validated before it is stored, so every
//! bundle a PDP can fetch is one it can load. Dry-run against past decisions and bundle
//! signing come next.

mod grpc;
mod store;

pub use grpc::{router, Access, BreakGlassReporter};
pub use store::{
    BreakGlassEvent, BreakGlassOutcome, BreakGlassRequest, BundleStatus, EnforcementSetting, ModelRoutingSetting, OutageFallbackSetting,
    PolicyBundle, PolicyStore, StoreError, AUTOMATED_CONTAINMENTS_SCOPE, INSTALL_SCOPE,
};
