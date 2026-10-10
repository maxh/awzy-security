//! Generated types for awzy-authz's gRPC APIs, plus conversions to and from `awzy-model`:
//! - `awzy.authz.v1`: the decision API enforcement points call.
//! - `awzy.policy.v1`: versioned policy bundles.

pub mod authz {
    pub mod v1 {
        tonic::include_proto!("awzy.authz.v1");
    }
}

pub mod policy {
    pub mod v1 {
        tonic::include_proto!("awzy.policy.v1");
    }
}

mod convert;

pub use prost_types;

pub use convert::{entities_to_json, entity_from_json, json_to_struct, struct_to_json, ConvertError};
