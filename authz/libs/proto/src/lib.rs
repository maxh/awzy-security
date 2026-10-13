//! Generated types for awzy-authz's gRPC APIs, plus conversions to and from `awzy-model`:
//! - `awzy.authz.v1`: the decision API enforcement points call.
//! - `awzy.policy.v1`: versioned policy bundles.
//! - `awzy.challenge.v1`: challenges and containments.
//! - `awzy.pdp.v1`: what a running PDP passes a starting one.

pub mod authz {
    pub mod v1 {
        tonic::include_proto!("awzy.authz.v1");
    }
}

pub mod challenge {
    pub mod v1 {
        tonic::include_proto!("awzy.challenge.v1");
    }
}

pub mod pdp {
    pub mod v1 {
        tonic::include_proto!("awzy.pdp.v1");
    }
}

pub mod policy {
    pub mod v1 {
        tonic::include_proto!("awzy.policy.v1");
    }
}

mod convert;

pub use prost_types;

pub use convert::{
    actor_from_wire, actor_to_wire, challenges_from_wire, challenges_to_wire, entities_to_json, entity_from_json,
    json_to_struct, struct_to_json, ConvertError,
};
