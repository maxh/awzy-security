//! Enforcement modes (ADR 0007): the install's mode, its history, the would-deny rates that tell
//! an admin whether it's safe to enforce, and the switch.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject};
use awzy_authz_proto::{
    authz::v1::EnforcementMode as WireMode,
    policy::v1::{EnforcementSetting, GetEnforcementRequest, ListEnforcementChangesRequest, SetEnforcementRequest},
};
use awzy_model::EnforcementMode;
use awzy_portal::{Actor, UtcTimestamp};
use chrono::DateTime;

use crate::{
    access::{require, PortalAction::*, PortalArea},
    clickhouse, decisions, policy_client, PolicyClient,
};

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(name = "EnforcementMode")]
pub enum Mode {
    /// Decisions are recorded but not applied: enforcement points allow.
    Monitor,
    /// Enforcement points apply the verdict.
    Enforce,
}

impl Mode {
    pub fn parse(s: &str) -> Self {
        EnforcementMode::parse(s).unwrap_or_default().into()
    }
}

impl From<EnforcementMode> for Mode {
    fn from(m: EnforcementMode) -> Self {
        match m {
            EnforcementMode::Monitor => Mode::Monitor,
            EnforcementMode::Enforce => Mode::Enforce,
        }
    }
}

impl From<Mode> for EnforcementMode {
    fn from(m: Mode) -> Self {
        match m {
            Mode::Monitor => EnforcementMode::Monitor,
            Mode::Enforce => EnforcementMode::Enforce,
        }
    }
}

// ---- Operations, shared with the MCP tools.

/// The install's current setting. awzy-policy answers monitor for a scope never switched.
pub async fn current(mut policy: PolicyClient) -> anyhow::Result<EnforcementSetting> {
    policy
        .get_enforcement(GetEnforcementRequest { scope: String::new() })
        .await?
        .into_inner()
        .setting
        .ok_or_else(|| anyhow::anyhow!("awzy-policy returned no enforcement setting"))
}

pub async fn history(mut policy: PolicyClient, limit: i32) -> anyhow::Result<Vec<EnforcementSetting>> {
    let req = ListEnforcementChangesRequest { scope: String::new(), limit };
    Ok(policy.list_enforcement_changes(req).await?.into_inner().changes)
}

pub async fn set(
    mut policy: PolicyClient,
    mode: EnforcementMode,
    actor: &awzy_model::Actor,
) -> anyhow::Result<EnforcementSetting> {
    let req = SetEnforcementRequest {
        scope: String::new(),
        mode: WireMode::from(mode).into(),
        changed_by_actor: Some(actor.clone().into()),
        ..Default::default()
    };
    policy
        .set_enforcement(req)
        .await?
        .into_inner()
        .setting
        .ok_or_else(|| anyhow::anyhow!("awzy-policy returned no enforcement setting"))
}

// ---- GraphQL

/// One enforcement setting: the current one, or a past switch.
pub struct Setting(pub EnforcementSetting);

#[Object(name = "EnforcementSetting")]
impl Setting {
    async fn mode(&self) -> Mode {
        EnforcementMode::from(self.0.mode()).into()
    }

    /// Null when the install has never been switched (new installs start in MONITOR).
    async fn changed_by(&self) -> Option<Actor> {
        Actor::from_text(&self.0.changed_by)
    }

    async fn changed_at(&self) -> Option<UtcTimestamp> {
        self.0
            .changed_at
            .as_ref()
            .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
            .map(UtcTimestamp)
    }
}

/// What enforcing would have changed, over a window of logged decisions.
#[derive(SimpleObject)]
pub struct EnforcementSummary {
    pub since: UtcTimestamp,
    pub total: u64,
    /// Decisions made in MONITOR mode.
    pub monitored: u64,
    /// MONITOR-mode decisions that would have been denied.
    pub would_deny: u64,
    /// MONITOR-mode decisions that would have been challenged.
    pub would_challenge: u64,
    pub enforced_deny: u64,
    pub enforced_challenge: u64,
}

#[derive(Default)]
pub struct EnforcementQuery;

#[Object]
impl EnforcementQuery {
    /// The install's enforcement mode.
    async fn enforcement(&self, ctx: &Context<'_>) -> Result<Setting> {
        require(ctx, View, PortalArea::Enforcement).await?;
        Ok(Setting(current(policy_client(ctx)?).await?))
    }

    /// Every switch of the install's mode, newest first.
    async fn enforcement_history(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
    ) -> Result<Vec<Setting>> {
        require(ctx, View, PortalArea::Enforcement).await?;
        Ok(history(policy_client(ctx)?, first).await?.into_iter().map(Setting).collect())
    }

    /// Would-deny and would-challenge counts since `since` (default 24 hours, at most 30 days).
    async fn enforcement_summary(&self, ctx: &Context<'_>, since: Option<UtcTimestamp>) -> Result<EnforcementSummary> {
        require(ctx, View, PortalArea::Enforcement).await?;
        let (since, c) = decisions::mode_counts(&clickhouse(ctx)?, since.map(|t| t.0)).await?;
        Ok(EnforcementSummary {
            since: UtcTimestamp(since),
            total: c.total,
            monitored: c.monitored,
            would_deny: c.would_deny,
            would_challenge: c.would_challenge,
            enforced_deny: c.enforced_deny,
            enforced_challenge: c.enforced_challenge,
        })
    }
}

#[derive(InputObject)]
pub struct SetEnforcementModeInput {
    pub mode: Mode,
}

#[derive(SimpleObject)]
pub struct SetEnforcementModePayload {
    pub enforcement: Setting,
}

#[derive(Default)]
pub struct EnforcementMutation;

#[Object]
impl EnforcementMutation {
    /// Switches the install between MONITOR and ENFORCE. PDPs pick it up within about 10 seconds.
    /// TODO: require a security-key challenge for the switch to ENFORCE (awzy-challenge).
    async fn set_enforcement_mode(
        &self,
        ctx: &Context<'_>,
        input: SetEnforcementModeInput,
    ) -> Result<SetEnforcementModePayload> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        let setting = set(policy_client(ctx)?, input.mode.into(), &viewer.actor()).await?;
        Ok(SetEnforcementModePayload { enforcement: Setting(setting) })
    }
}
