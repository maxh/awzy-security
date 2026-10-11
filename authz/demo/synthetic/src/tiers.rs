//! The demo MCP servers' tiers, as a demo install's client portal would set them in its outage
//! settings (ADR 0089): larkspur-drive's document tools internal, larkspur-admin's operations
//! confidential or crown jewels, and anything else on admin crown jewels.

use awzy_authz_proto::policy::v1::{
    policy_service_client::PolicyServiceClient, GetOutageFallbackRequest, SetOutageFallbackRequest,
};
use awzy_model::OutageFallback;

use crate::activity::ADMIN_TOOLS;

pub fn demo_tiers() -> OutageFallback {
    let mut tiers = OutageFallback::default();
    tiers.servers.insert("larkspur-drive".into(), "internal".into());
    tiers.servers.insert("larkspur-admin".into(), "crown_jewels".into());
    for tool in ["read_document", "list_folder"] {
        tiers.tools.insert(format!("larkspur-drive/{tool}"), "internal".into());
    }
    for (tool, tier) in ADMIN_TOOLS {
        tiers.tools.insert(format!("larkspur-admin/{tool}"), tier.to_string());
    }
    tiers
}

/// Sets [`demo_tiers`] in awzy-policy at `addr`, unless a setting exists, so what someone changed
/// in the demo's portal stands. Returns whether it set them.
pub async fn seed(addr: &str) -> anyhow::Result<bool> {
    let channel = tonic::transport::Channel::from_shared(addr.to_string())?.connect_lazy();
    let mut policy = PolicyServiceClient::new(awzy_service_auth::grpc_channel(channel, "awzy-policy"));
    let current = policy.get_outage_fallback(GetOutageFallbackRequest { known_version: -1 }).await?.into_inner();
    if current.fallback.is_some_and(|f| f.version > 0) {
        return Ok(false);
    }
    let tiers = demo_tiers();
    let by = awzy_model::Actor::parse("ServiceAccount::awzy-synthetic");
    policy
        .set_outage_fallback(SetOutageFallbackRequest {
            tools: tiers.tools.into_iter().collect(),
            servers: tiers.servers.into_iter().collect(),
            fail_open_tiers: vec![],
            changed_by_actor: Some(by.into()),
        })
        .await?;
    Ok(true)
}
