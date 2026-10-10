//! Who may approve a challenge: the PDP decides, as `approve` on the challenged resource, so
//! approvers are set in Cedar like any other access (the base policies let the security team).

use awzy_authz_proto::authz::v1::{authorizer_client::AuthorizerClient, CheckRequest, EntityRef, Verdict};
use awzy_model::entity_type;
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
    /// These people may approve anything. For tests.
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

    /// Ok when the PDP allows `approver` (a person's ID) to approve `resource`. The check is
    /// always enforced, whatever the install's mode, as the portal's own checks are (ADR 0020).
    pub async fn check(&self, approver: &str, resource: &awzy_model::EntityRef) -> Result<(), ApprovalError> {
        let mut client = match self {
            Self::Pdp(client) => client.clone(),
            Self::Off => return Err(ApprovalError::Unavailable("awzy-challenge has no PDP (AWZY_PDP_ADDR)".into())),
            #[cfg(test)]
            Self::Only(people) => {
                return if people.iter().any(|p| p == approver) {
                    Ok(())
                } else {
                    Err(ApprovalError::Refused(format!("{approver} may not approve")))
                }
            }
        };
        let res = client
            .check(CheckRequest {
                subject: Some(EntityRef { r#type: entity_type::HUMAN.into(), id: approver.into() }),
                action: APPROVE_ACTION.into(),
                resource: Some(resource.clone().into()),
                human_present: Some(true),
                always_enforced: true,
                ..Default::default()
            })
            .await
            .map_err(|s| ApprovalError::Unavailable(s.message().to_string()))?
            .into_inner();
        match res.verdict() {
            Verdict::Allow => Ok(()),
            _ => Err(ApprovalError::Refused(format!(
                "policy doesn't let {approver} approve access to {}::{:?} (decision {})",
                resource.kind, resource.id, res.decision_id
            ))),
        }
    }
}
