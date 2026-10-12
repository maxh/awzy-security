//! Enforcement modes (ADR 0007): the install's mode and automated containments' (ADR 0066), their
//! history, the would-deny rates that tell an admin whether it's safe to enforce, and the switch.

use async_graphql::{connection::Connection, Context, Enum, InputObject, Object, Result, SimpleObject};
use awzy_authz_proto::{
    authz::v1::EnforcementMode as WireMode,
    policy::v1::{EnforcementSetting, GetEnforcementRequest, ListEnforcementChangesRequest, SetEnforcementRequest},
};
use awzy_model::{ChallengeId, EnforcementMode};
use awzy_portal::{
    connection::{page, MAX_PAGE},
    Actor, Qid, UtcTimestamp,
};
use chrono::DateTime;

use crate::{
    access::{self, require, PortalAction::*, PortalArea, Surface},
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

/// What a mode applies to.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Default)]
pub enum EnforcementScope {
    /// The install's verdicts: whether enforcement points apply them.
    #[default]
    Install,
    /// awzy-reasoner's automated containments: whether they're applied or only recorded.
    AutomatedContainments,
}

impl EnforcementScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => awzy_model::INSTALL_SCOPE,
            Self::AutomatedContainments => awzy_model::AUTOMATED_CONTAINMENTS_SCOPE,
        }
    }
}

/// What a scope below the install covers (ADR 0109).
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum ScopeKind {
    /// One Cedar action, such as `write` or an app's `view_customer`.
    Action,
    /// One MCP tool behind awzy-gateway, `<server>/<tool>`.
    Tool,
}

/// An action or a tool, to give a mode of its own.
#[derive(InputObject)]
pub struct EnforcementTarget {
    pub kind: ScopeKind,
    /// The action's name, or the tool's `<server>/<tool>`.
    pub name: String,
}

impl EnforcementTarget {
    /// The scope awzy-policy keeps its mode under, if the name is one.
    fn scope(&self) -> Result<String> {
        let prefix = match self.kind {
            ScopeKind::Action => awzy_model::ACTION_SCOPE_PREFIX,
            ScopeKind::Tool => awzy_model::TOOL_SCOPE_PREFIX,
        };
        let scope = format!("{prefix}{}", self.name.trim());
        if awzy_model::is_valid_scope(&scope) {
            Ok(scope)
        } else {
            Err(awzy_portal::ErrorCode::InvalidInput.error(match self.kind {
                ScopeKind::Action => "an action is named like a Cedar identifier, such as write or view_customer",
                ScopeKind::Tool => "a tool is named <server>/<tool>, such as larkspur-drive/read_document",
            }))
        }
    }
}

// ---- Operations, shared with the MCP tools.

/// A scope's current setting. awzy-policy answers monitor for a scope never switched.
pub async fn current(mut policy: PolicyClient, scope: EnforcementScope) -> anyhow::Result<EnforcementSetting> {
    policy
        .get_enforcement(GetEnforcementRequest { scope: scope.as_str().into() })
        .await?
        .into_inner()
        .setting
        .ok_or_else(|| anyhow::anyhow!("awzy-policy returned no enforcement setting"))
}

pub async fn history(
    mut policy: PolicyClient,
    scope: EnforcementScope,
    limit: i32,
) -> anyhow::Result<Vec<EnforcementSetting>> {
    let req = ListEnforcementChangesRequest { scope: scope.as_str().into(), limit };
    Ok(policy.list_enforcement_changes(req).await?.into_inner().changes)
}

/// Every action and tool scope that has been switched, at its current setting.
pub async fn scoped(mut policy: PolicyClient) -> anyhow::Result<Vec<EnforcementSetting>> {
    let req = awzy_authz_proto::policy::v1::ListEnforcementSettingsRequest {};
    Ok(policy.list_enforcement_settings(req).await?.into_inner().settings)
}

pub async fn set(
    policy: PolicyClient,
    scope: EnforcementScope,
    mode: EnforcementMode,
    actor: &awzy_model::Actor,
) -> anyhow::Result<EnforcementSetting> {
    set_scope(policy, scope.as_str(), mode, actor).await
}

/// Switches any scope, named as awzy-policy keeps it.
pub async fn set_scope(
    mut policy: PolicyClient,
    scope: &str,
    mode: EnforcementMode,
    actor: &awzy_model::Actor,
) -> anyhow::Result<EnforcementSetting> {
    let req = SetEnforcementRequest {
        scope: scope.into(),
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

    /// Null when the scope has never been switched (every scope starts in MONITOR).
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

/// An action's or a tool's own mode (ADR 0109), and what enforcing it would change.
pub struct ScopedEnforcement {
    setting: EnforcementSetting,
    counts: decisions::ModeCounts,
}

#[Object]
impl ScopedEnforcement {
    async fn kind(&self) -> ScopeKind {
        if self.setting.scope.starts_with(awzy_model::TOOL_SCOPE_PREFIX) {
            ScopeKind::Tool
        } else {
            ScopeKind::Action
        }
    }

    /// The action's name, or the tool's `<server>/<tool>`.
    async fn name(&self) -> &str {
        let s = &self.setting.scope;
        s.strip_prefix(awzy_model::TOOL_SCOPE_PREFIX)
            .or_else(|| s.strip_prefix(awzy_model::ACTION_SCOPE_PREFIX))
            .unwrap_or(s)
    }

    async fn setting(&self) -> Setting {
        Setting(self.setting.clone())
    }

    /// Its checks in the last 24 hours.
    async fn total(&self) -> u64 {
        self.counts.total
    }

    /// Of those in MONITOR, the ones that would have been denied.
    async fn would_deny(&self) -> u64 {
        self.counts.would_deny
    }

    /// Of those in MONITOR, the ones that would have been challenged.
    async fn would_challenge(&self) -> u64 {
        self.counts.would_challenge
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
    /// A scope's enforcement mode: the install's by default.
    async fn enforcement(&self, ctx: &Context<'_>, #[graphql(default)] scope: EnforcementScope) -> Result<Setting> {
        require(ctx, View, PortalArea::Enforcement).await?;
        Ok(Setting(current(policy_client(ctx)?, scope).await?))
    }

    /// Every switch of a scope's mode, newest first.
    async fn enforcement_history(
        &self,
        ctx: &Context<'_>,
        #[graphql(default)] scope: EnforcementScope,
        after: Option<String>,
        first: Option<i32>,
    ) -> Result<Connection<String, Setting>> {
        require(ctx, View, PortalArea::Enforcement).await?;
        let changes = history(policy_client(ctx)?, scope, MAX_PAGE as i32).await?;
        // A change has no ID of its own; when it was made is unique within one scope's history.
        let key =
            |s: &Setting| s.0.changed_at.as_ref().map(|t| format!("{}.{:09}", t.seconds, t.nanos)).unwrap_or_default();
        page(changes.into_iter().map(Setting).collect(), key, after, first)
    }

    /// Actions and tools with a mode of their own (ADR 0109), which a check takes over the
    /// install's: its tool's first, then its action's.
    async fn enforcement_scopes(&self, ctx: &Context<'_>) -> Result<Vec<ScopedEnforcement>> {
        require(ctx, View, PortalArea::Enforcement).await?;
        let ch = clickhouse(ctx)?;
        let mut out = Vec::new();
        for setting in scoped(policy_client(ctx)?).await? {
            let action = setting.scope.strip_prefix(awzy_model::ACTION_SCOPE_PREFIX);
            let tool = setting.scope.strip_prefix(awzy_model::TOOL_SCOPE_PREFIX);
            let (_, counts) = decisions::mode_counts_for(&ch, None, action.or(tool.map(|_| "call")), tool).await?;
            out.push(ScopedEnforcement { setting, counts });
        }
        Ok(out)
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
    /// The install's by default.
    #[graphql(default)]
    pub scope: EnforcementScope,
}

#[derive(SimpleObject)]
pub struct SetEnforcementModePayload {
    /// The scope's setting once switched. Null while a challenge stands in the way.
    pub enforcement: Option<Setting>,
    /// The security-key challenge to pass first, when the switch needs one: answer it, then switch
    /// again within its grant.
    pub challenge_qid: Option<Qid<ChallengeId>>,
}

/// Whether policy lets `viewer` change what's enforced now (`switch_enforcement`, ADR 0080):
/// `Some` names the challenge to pass first, as the base policies ask for a security key.
pub(crate) async fn switch_challenge(ctx: &Context<'_>, viewer: &access::Viewer) -> Result<Option<Qid<ChallengeId>>> {
    action_challenge(ctx, viewer, "switch_enforcement", "changing what's enforced").await
}

/// Whether policy lets `viewer` do `action` on the enforcement area now, an action the portal asks
/// about only when someone does it, so policy can challenge it: `Some` names the challenge to pass
/// first. `what` says what's being done, for the error when no challenge could be opened.
pub(crate) async fn action_challenge(
    ctx: &Context<'_>,
    viewer: &access::Viewer,
    action: &'static str,
    what: &str,
) -> Result<Option<Qid<ChallengeId>>> {
    let services = ctx.data::<crate::Services>()?;
    match access::authorize_action(services, viewer, Surface::Portal, action, PortalArea::Enforcement).await {
        Ok(()) => Ok(None),
        Err(access::AccessError::Challenged { challenge_qid: Some(qid), .. }) => {
            Ok(Some(Qid(qid.parse::<ChallengeId>()?)))
        }
        Err(access::AccessError::Challenged { challenges, decision, .. }) => Err(format!(
            "{what} needs a {challenges} challenge, and none could be opened: is awzy-challenge reachable? \
             (decision {decision})"
        )
        .into()),
        Err(e) => Err(e.into()),
    }
}

#[derive(InputObject)]
pub struct SetScopedEnforcementModeInput {
    pub target: EnforcementTarget,
    pub mode: Mode,
}

#[derive(Default)]
pub struct EnforcementMutation;

#[Object]
impl EnforcementMutation {
    /// Switches a scope between MONITOR and ENFORCE. Every PDP picks up the install's as it's made;
    /// awzy-reasoner reads automated containments' on its next run. Policy can ask for a security
    /// key first, as the base policies do (`switch_enforcement`): then nothing is switched, and the
    /// payload names the challenge to pass before switching again.
    async fn set_enforcement_mode(
        &self,
        ctx: &Context<'_>,
        input: SetEnforcementModeInput,
    ) -> Result<SetEnforcementModePayload> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        if let Some(challenge_qid) = switch_challenge(ctx, &viewer).await? {
            return Ok(SetEnforcementModePayload { enforcement: None, challenge_qid: Some(challenge_qid) });
        }
        let setting = set(policy_client(ctx)?, input.scope, input.mode.into(), &viewer.actor()).await?;
        Ok(SetEnforcementModePayload { enforcement: Some(Setting(setting)), challenge_qid: None })
    }

    /// Gives an action or a tool a mode of its own (ADR 0109), so, for example, agents' writes
    /// are enforced while their reads stay in MONITOR. Policy can ask for a security key first, as
    /// for the install's (`switch_enforcement`).
    async fn set_scoped_enforcement_mode(
        &self,
        ctx: &Context<'_>,
        input: SetScopedEnforcementModeInput,
    ) -> Result<SetEnforcementModePayload> {
        let viewer = require(ctx, Administer, PortalArea::Enforcement).await?;
        let scope = input.target.scope()?;
        if let Some(challenge_qid) = switch_challenge(ctx, &viewer).await? {
            return Ok(SetEnforcementModePayload { enforcement: None, challenge_qid: Some(challenge_qid) });
        }
        let setting = set_scope(policy_client(ctx)?, &scope, input.mode.into(), &viewer.actor()).await?;
        Ok(SetEnforcementModePayload { enforcement: Some(Setting(setting)), challenge_qid: None })
    }
}
