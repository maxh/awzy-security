//! Generates Rust code for the protos with protox (a pure-Rust protobuf compiler), so
//! neither Cargo nor Bazel needs `protoc`.

use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let fds = protox::compile(["awzy/authz/v1/authz.proto", "awzy/policy/v1/policy.proto"], [&root])?;
    tonic_prost_build::configure().compile_fds(fds)?;
    println!("cargo:rerun-if-changed=awzy");
    Ok(())
}
