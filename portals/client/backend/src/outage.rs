//! The outage fallback setting (ADR 0089): each MCP tool's sensitivity tier, which every check
//! sends the PDP, and which tiers may replay a recent allow while the PDP is unreachable. Kept and
//! signed by awzy-policy; awzy-gateway verifies it before using it.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject, ID};
use awzy_authz_proto::policy::v1::{
    GetOutageFallbackRequest, OutageFallback as WireFallback, SetOutageFallbackRequest,
};
use awzy_model::OutageFallback;
use awzy_portal::{Actor, UtcTimestamp};
use chrono::DateTime;

use crate::{
    access::{require, PortalAction::*, PortalArea},
    enforcement::switch_challenge,
    policy_client,
};

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum SensitivityTier {
    Public,
    Internal,
    Confidential,
    CrownJewels,
}

impl SensitivityTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::CrownJewels => "crown_jewels",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [Self::Public, Self::Internal, Self::Confidential, Self::CrownJewels].into_iter().find(|t| t.as_str() == s)
    }
}

/// A tool's tier.
#[derive(SimpleObject, InputObject, Clone)]
#[graphql(input_name = "ToolTierInput")]
pub struct ToolTier {
    /// `<server>/<tool>`, e.g. `drive/search`.
    pub tool: String,
    pub tier: SensitivityTier,
}

/// The tier of a server's tools that aren't listed by name.
#[derive(SimpleObject, InputObject, Clone)]
#[graphql(input_name = "ServerTierInput")]
pub struct ServerTier {
    pub server: String,
    pub tier: SensitivityTier,
}

pub struct OutageFallbackSetting(WireFallback);

#[Object(name = "OutageFallback")]
impl OutageFallbackSetting {
    /// 0 when never set: every tool is unclassified (confidential) and fails closed.
    async fn version(&self) -> i64 {
        self.0.version
    }

    /// Tools classified by name. Only these can fail open.
    async fn tools(&self) -> Vec<ToolTier> {
        let mut tools: Vec<ToolTier> = self
            .0
            .tools
            .iter()
            .filter_map(|(tool, t)| Some(ToolTier { tool: tool.clone(), tier: SensitivityTier::parse(t)? }))
            .collect();
        tools.sort_by(|a, b| a.tool.cmp(&b.tool));
        tools
    }

    async fn servers(&self) -> Vec<ServerTier> {
        let mut servers: Vec<ServerTier> = self
            .0
            .servers
            .iter()
            .filter_map(|(server, t)| Some(ServerTier { server: server.clone(), tier: SensitivityTier::parse(t)? }))
            .collect();
        servers.sort_by(|a, b| a.server.cmp(&b.server));
        servers
    }

    /// Tiers whose listed tools replay a recent allow while the PDP is unreachable.
    async fn fail_open_tiers(&self) -> Vec<SensitivityTier> {
        self.0.fail_open_tiers.iter().filter_map(|t| SensitivityTier::parse(t)).collect()
    }

    async fn changed_by(&self) -> Option<Actor> {
        awzy_authz_proto::actor_from_wire(self.0.changed_by_actor.clone(), "")
            .and_then(|a| Actor::from_text(&a.to_string()))
    }

    async fn changed_at(&self) -> Option<UtcTimestamp> {
        self.0
            .changed_at
            .as_ref()
            .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
            .map(UtcTimestamp)
    }
}

#[derive(Default)]
pub struct OutageQuery;

#[Object]
impl OutageQuery {
    /// What awzy-gateway does while the PDP is unreachable: each tool's tier, and which tiers
    /// fail open.
    async fn outage_fallback(&self, ctx: &Context<'_>) -> Result<OutageFallbackSetting> {
        require(ctx, View, PortalArea::Enforcement).await?;
        let mut policy = policy_client(ctx)?;
        let res = policy.get_outage_fallback(GetOutageFallbackRequest { known_version: -1 }).await?.into_inner();
        Ok(OutageFallbackSetting(res.fallback.ok_or("awzy-policy returned no outage fallback setting")?))
    }
}

#[derive(InputObject)]
pub struct SetOutageFallbackInput {
    pub tools: Vec<ToolTier>,
    pub servers: Vec<ServerTier>,
    /// Never CONFIDENTIAL or CROWN_JEWELS.
    pub fail_open_tiers: Vec<SensitivityTier>,
}

#[derive(SimpleObject)]
pub struct SetOutageFallbackPayload {
    /// The new setting; null when a challenge must be passed first.
    pub outage_fallback: Option<OutageFallbackSetting>,
    /// The challenge to pass, then save again.
    pub challenge_qid: Option<ID>,
}

#[derive(Default)]
pub struct OutageMutation;

#[Object]
impl OutageMutation {
    /// Replaces the outage fallback setting. Like switching enforcement, policy can ask for a
    /// security key first: then nothing changes, and the payload names the challenge to pass.
    async fn set_outage_fallback(
        &self,
        ctx: &Context<'_>,
        input: SetOutageFallbackInput,
    ) -> Result<SetOutageFallbackPayload> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        let fallback = OutageFallback {
            tools: input.tools.into_iter().map(|t| (t.tool.trim().to_string(), t.tier.as_str().to_string())).collect(),
            servers: input
                .servers
                .into_iter()
                .map(|s| (s.server.trim().to_string(), s.tier.as_str().to_string()))
                .collect(),
            fail_open_tiers: input.fail_open_tiers.into_iter().map(|t| t.as_str().to_string()).collect(),
        };
        // Before any challenge, so a mistake is reported without a touch of the key.
        fallback.validate()?;
        if let Some(challenge_qid) = switch_challenge(ctx, &viewer).await? {
            return Ok(SetOutageFallbackPayload { outage_fallback: None, challenge_qid: Some(challenge_qid) });
        }
        let req = SetOutageFallbackRequest {
            tools: fallback.tools.into_iter().collect(),
            servers: fallback.servers.into_iter().collect(),
            fail_open_tiers: fallback.fail_open_tiers.into_iter().collect(),
            changed_by_actor: Some(viewer.actor().into()),
        };
        let setting = policy_client(ctx)?.set_outage_fallback(req).await?.into_inner().fallback;
        Ok(SetOutageFallbackPayload {
            outage_fallback: Some(OutageFallbackSetting(
                setting.ok_or("awzy-policy returned no outage fallback setting")?,
            )),
            challenge_qid: None,
        })
    }
}
