//! Publishes a policy bundle straight to awzy-policy, without the portal.
//!
//!   awzy-publish --endpoint http://awzy-policy:8080 --by "install" \
//!     [--schema awzy.cedarschema] [--policies policies.cedar]
//!
//! For the install's first bundle, and for upgrades that change the portal's own rules
//! (ADR 0020), when the active bundle doesn't yet let anyone use the portal. Without `--schema`
//! or `--policies` it publishes the base schema and policies built into this release.

use anyhow::{bail, Context};
use awzy_authz_proto::policy::v1::{policy_service_client::PolicyServiceClient, CreateDraftRequest, PublishRequest};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn read_or(path: Option<String>, default: &str) -> anyhow::Result<String> {
    match path {
        Some(p) => std::fs::read_to_string(&p).with_context(|| format!("reading {p}")),
        None => Ok(default.to_string()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(endpoint), Some(by)) = (arg(&args, "--endpoint"), arg(&args, "--by")) else {
        bail!("usage: awzy-publish --endpoint <awzy-policy URL> --by <who> [--schema <file>] [--policies <file>]");
    };
    let schema = read_or(arg(&args, "--schema"), awzy_pdp::DEFAULT_SCHEMA)?;
    let policies = read_or(arg(&args, "--policies"), awzy_pdp::DEFAULT_POLICIES)?;

    let mut client = PolicyServiceClient::connect(endpoint).await?;
    let draft = client
        .create_draft(CreateDraftRequest { schema, policies, created_by: by.clone() })
        .await
        .map_err(|s| anyhow::anyhow!("{}", s.message()))?
        .into_inner()
        .bundle
        .context("awzy-policy returned no draft")?;
    let bundle = client
        .publish(PublishRequest { policy_bundle_qid: draft.qid, published_by: by })
        .await?
        .into_inner()
        .bundle
        .context("awzy-policy returned no bundle")?;
    println!("Published version {} ({}). PDPs pick it up within about 10 seconds.", bundle.version, bundle.qid);
    Ok(())
}
