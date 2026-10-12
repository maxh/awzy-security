//! Apps an admin trusts to sign tokens for their own agents (ADR 0110), on the Directory page.

use async_graphql::{Context, InputObject, Object, Result, SimpleObject};
use awzy_portal::{Actor, UtcTimestamp};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    access::{require, PortalAction, PortalArea},
    directory::context,
};

#[derive(Deserialize)]
struct WireIssuer {
    name: String,
    issuer: String,
    jwks_url: String,
    audience: String,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_by: String,
    updated_at: DateTime<Utc>,
}

/// An app that signs tokens for its own agents: the service accounts whose ID starts with its name
/// and a dot.
#[derive(SimpleObject)]
pub struct TokenIssuer {
    /// The app's name: the `sub` of the tokens it signs for itself, to register its agents.
    pub name: String,
    /// The `iss` its tokens carry.
    pub issuer: String,
    /// Where its signing keys are published.
    pub jwks_url: String,
    /// The `aud` its tokens must carry.
    pub audience: String,
    /// What its agents' IDs start with.
    pub agent_prefix: String,
    pub created_by: Option<Actor>,
    pub created_at: UtcTimestamp,
    pub updated_by: Option<Actor>,
    pub updated_at: UtcTimestamp,
}

impl From<WireIssuer> for TokenIssuer {
    fn from(w: WireIssuer) -> Self {
        Self {
            agent_prefix: format!("{}.", w.name),
            name: w.name,
            issuer: w.issuer,
            jwks_url: w.jwks_url,
            audience: w.audience,
            created_by: Actor::from_text(&w.created_by),
            created_at: UtcTimestamp(w.created_at),
            updated_by: Actor::from_text(&w.updated_by),
            updated_at: UtcTimestamp(w.updated_at),
        }
    }
}

#[derive(Default)]
pub struct TokenIssuerQuery;

#[Object]
impl TokenIssuerQuery {
    /// Apps trusted to sign tokens for their own agents, besides the IdP.
    async fn token_issuers(&self, ctx: &Context<'_>) -> Result<Vec<TokenIssuer>> {
        require(ctx, PortalAction::View, PortalArea::Directory).await?;
        #[derive(Deserialize)]
        struct Wire {
            token_issuers: Vec<WireIssuer>,
        }
        let w: Wire = context(ctx)?.get("/v1/token-issuers").await?;
        Ok(w.token_issuers.into_iter().map(Into::into).collect())
    }
}

#[derive(InputObject, Serialize)]
pub struct RegisterTokenIssuerInput {
    /// Lowercase letters, digits, and dashes. Its agents' IDs start with it and a dot.
    #[serde(skip)]
    pub name: String,
    pub issuer: String,
    /// https://
    pub jwks_url: String,
    pub audience: String,
}

#[derive(SimpleObject)]
pub struct RegisterTokenIssuerPayload {
    pub token_issuer: TokenIssuer,
}

#[derive(InputObject)]
pub struct RemoveTokenIssuerInput {
    pub name: String,
}

#[derive(SimpleObject)]
pub struct RemoveTokenIssuerPayload {
    pub removed_name: String,
}

#[derive(Default)]
pub struct TokenIssuerMutation;

#[Object]
impl TokenIssuerMutation {
    /// Trusts an app to sign tokens for its own agents, or changes how. Enforcement points accept
    /// its tokens within seconds.
    async fn register_token_issuer(
        &self,
        ctx: &Context<'_>,
        input: RegisterTokenIssuerInput,
    ) -> Result<RegisterTokenIssuerPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Directory).await?;
        let (status, res) = context(ctx)?
            .call(
                "PUT",
                &format!("/v1/token-issuers/{}", awzy_http::encode(&input.name)),
                Some(&viewer.actor().to_string()),
                Some(serde_json::to_value(&input)?),
            )
            .await?;
        if status != 200 {
            return Err(String::from_utf8_lossy(&res).into_owned().into());
        }
        #[derive(Deserialize)]
        struct Wire {
            token_issuer: WireIssuer,
        }
        let w: Wire = serde_json::from_slice(&res)?;
        Ok(RegisterTokenIssuerPayload { token_issuer: w.token_issuer.into() })
    }

    /// Stops trusting an app's tokens. Its agents stay registered until they're removed.
    async fn remove_token_issuer(
        &self,
        ctx: &Context<'_>,
        input: RemoveTokenIssuerInput,
    ) -> Result<RemoveTokenIssuerPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Directory).await?;
        let (status, res) = context(ctx)?
            .call(
                "DELETE",
                &format!("/v1/token-issuers/{}", awzy_http::encode(&input.name)),
                Some(&viewer.actor().to_string()),
                None,
            )
            .await?;
        match status {
            204 => Ok(RemoveTokenIssuerPayload { removed_name: input.name }),
            _ => Err(String::from_utf8_lossy(&res).into_owned().into()),
        }
    }
}
