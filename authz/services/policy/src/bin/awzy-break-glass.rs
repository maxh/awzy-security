//! Break-glass rollback, for when a bad policy bundle locks admins out of the client portal.
//!
//!   awzy-break-glass --endpoint http://awzy-policy:8080 --key admin.pem \
//!     --reason "forbid-all bundle locked us out" [--to-version 12]
//!
//! Signs the request with an admin key enrolled at install time and sends it straight to
//! awzy-policy, so it works when the IdP, the PDP, or the portal is what's broken. Without
//! `--to-version` it restores the bundle published before the active one. Every use is recorded
//! and logged.

use anyhow::{bail, Context};
use awzy_authz_proto::policy::v1::{policy_service_client::PolicyServiceClient, BreakGlassRollbackRequest};
use awzy_signing::{break_glass_payload, BundleSigner};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(endpoint), Some(key), Some(reason)) =
        (arg(&args, "--endpoint"), arg(&args, "--key"), arg(&args, "--reason"))
    else {
        bail!("usage: awzy-break-glass --endpoint <awzy-policy URL> --key <admin key PEM> --reason <why> [--to-version N]");
    };
    let to_version: i64 =
        arg(&args, "--to-version").map(|v| v.parse()).transpose().context("--to-version")?.unwrap_or(0);
    let signer = BundleSigner::from_pem(&std::fs::read_to_string(&key).with_context(|| format!("reading {key}"))?)?;

    let requested_at_unix = chrono::Utc::now().timestamp();
    let nonce = uuid::Uuid::now_v7().to_string();
    let signature = signer.sign_payload(&break_glass_payload(to_version, requested_at_unix, &nonce, &reason));
    let req = BreakGlassRollbackRequest {
        to_version,
        requested_at_unix,
        nonce,
        reason,
        signature,
        key_id: signer.key_id().to_string(),
    };
    let res = PolicyServiceClient::connect(endpoint).await?.break_glass_rollback(req).await?.into_inner();
    let bundle = res.bundle.context("awzy-policy returned no bundle")?;
    println!(
        "Restored version {} as version {} ({}). Replaced version {}. PDPs pick it up within about 10 seconds.",
        res.restored_version, bundle.version, bundle.qid, res.replaced_version
    );
    Ok(())
}
