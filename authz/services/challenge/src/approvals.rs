//! Who may approve a challenge: the PDP decides, as `approve` on the challenged resource, so
//! approvers are set in Cedar like any other access (the base policies let the security team's
//! people). A service account, such as an overseer agent, approves where a policy names it
//! (ADR 0065).

use awzy_authz_proto::{
    authz::v1::{authorizer_client::AuthorizerClient, CheckRequest, Entity, Verdict},
    json_to_struct,
};
use awzy_model::{entity_type, EntityRef};
use serde_json::json;
use tonic::transport::Channel;

pub const APPROVE_ACTION: &str = "approve";

#[derive(Debug, thiserror::Error)]
pub enum ApprovalError {
    #[error("{0}")]
    Refused(String),
    /// No PDP configured, or it couldn't decide.
    #[error("can't check who may approve: {0}")]
    Unavailable(String),
}

pub enum Approvals {
    /// Asks the PDP at this channel.
    Pdp(AuthorizerClient<awzy_service_auth::AuthChannel>),
    /// No PDP configured: every approval is refused.
    Off,
    /// These people or service accounts (by ID) may approve anything. For tests.
    #[cfg(test)]
    Only(Vec<String>),
}

impl Approvals {
    /// Connects lazily, so the service starts before the PDP is up.
    pub fn pdp(endpoint: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(endpoint.to_string())?.connect_lazy();
        // With this service's token for the PDP (ADR 0055).
        Ok(Self::Pdp(AuthorizerClient::new(awzy_service_auth::grpc_channel(channel, "awzy-pdp"))))
    }

    /// Ok when the PDP allows `approver` (a person or a service account) to approve `resource`
    /// for `approving_for`, the person who answers for the challenged request. `entities` are the
    /// approver's as the caller validated them. The check is always enforced, whatever the
    /// install's mode, as the portal's own checks are (ADR 0020).
    pub async fn check(
        &self,
        approver: &EntityRef,
        resource: &EntityRef,
        approving_for: Option<&str>,
        entities: Vec<Entity>,
    ) -> Result<(), ApprovalError> {
        let mut client = match self {
            Self::Pdp(client) => client.clone(),
            Self::Off => return Err(ApprovalError::Unavailable("awzy-challenge has no PDP (AWZY_PDP_ADDR)".into())),
            #[cfg(test)]
            Self::Only(humans) => {
                return if humans.contains(&approver.id) {
                    Ok(())
                } else {
                    Err(ApprovalError::Refused(format!("{} may not approve", approver.id)))
                }
            }
        };
        let mut context = serde_json::Map::new();
        if let Some(human) = approving_for {
            context.insert("approving_for".into(), json!({ "__entity": { "type": entity_type::HUMAN, "id": human } }));
        }
        let res = client
            .check(CheckRequest {
                subject: Some(approver.clone().into()),
                action: APPROVE_ACTION.into(),
                resource: Some(resource.clone().into()),
                context: Some(json_to_struct(context)),
                always_enforced: true,
                entities,
                ..Default::default()
            })
            .await
            .map_err(|s| ApprovalError::Unavailable(s.message().to_string()))?
            .into_inner();
        match res.verdict() {
            Verdict::Allow => Ok(()),
            _ => Err(ApprovalError::Refused(format!(
                "policy doesn't let {} approve access to {}::{:?} (decision {})",
                approver.id, resource.kind, resource.id, res.decision_qid
            ))),
        }
    }
}
