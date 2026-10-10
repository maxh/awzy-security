//! Security keys (ADR 0059), through awzy-challenge: people enroll and remove their own, pass
//! verification challenges with them, and an admin can reset a lost one.
//!
//! The browser runs each WebAuthn ceremony: a `begin…` mutation returns the options for
//! `navigator.credentials`, and the matching `finish…` takes the response. Binary fields are
//! base64url. Every check happens in awzy-challenge; the portal only says who's signed in.

use async_graphql::{Context, InputObject, Object, Result, SimpleObject, ID};
use awzy_authz_proto::challenge::v1::{
    Assertion as WireAssertion, BeginKeyChangeRequest, BeginKeyRegistrationRequest, BeginVerificationRequest,
    FinishKeyRegistrationRequest, FinishVerificationRequest, ListSecurityKeysRequest, RemoveSecurityKeyRequest,
    ResetSecurityKeyRequest, SecurityKey as WireKey,
};
use awzy_portal::UtcTimestamp;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

use crate::{
    access::{require, PortalAction, PortalArea},
    challenges::{answering_person, client, grpc, time, Challenge, ChallengePayload},
};

fn bytes(field: &str, b64: &str) -> Result<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(b64.trim_end_matches('=')).map_err(|_| format!("{field} isn't base64url").into())
}

/// A person's security key.
pub struct SecurityKey(WireKey);

#[Object]
impl SecurityKey {
    /// QID: security_key_<uuid>.
    async fn id(&self) -> ID {
        ID(self.0.qid.clone())
    }

    async fn person_id(&self) -> &str {
        &self.0.person_id
    }

    /// What the person called it.
    async fn name(&self) -> &str {
        &self.0.name
    }

    /// The key that authorized adding this one; null for a person's first key.
    async fn added_with_qid(&self) -> Option<&str> {
        Some(self.0.added_with_qid.as_str()).filter(|s| !s.is_empty())
    }

    async fn created_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.created_at)
    }

    async fn last_used_at(&self) -> Option<UtcTimestamp> {
        time(&self.0.last_used_at)
    }
}

/// One WebAuthn ceremony for the browser to run.
#[derive(SimpleObject)]
pub struct WebAuthnCeremony {
    /// Pass back with the browser's response.
    pub ceremony_qid: ID,
    /// The options for `navigator.credentials.create()` or `.get()`, as WebAuthn's JSON (binary
    /// fields base64url).
    pub options_json: String,
}

/// A `navigator.credentials.get()` response.
#[derive(InputObject)]
pub struct AssertionInput {
    pub ceremony_qid: ID,
    /// `rawId`, base64url.
    pub credential_id: String,
    /// `response.clientDataJSON`, base64url.
    pub client_data_json: String,
    /// `response.authenticatorData`, base64url.
    pub authenticator_data: String,
    /// `response.signature`, base64url.
    pub signature: String,
}

impl AssertionInput {
    fn wire(self) -> Result<WireAssertion> {
        Ok(WireAssertion {
            ceremony_qid: self.ceremony_qid.0,
            credential_id: bytes("credentialId", &self.credential_id)?,
            client_data_json: bytes("clientDataJson", &self.client_data_json)?,
            authenticator_data: bytes("authenticatorData", &self.authenticator_data)?,
            signature: bytes("signature", &self.signature)?,
        })
    }
}

#[derive(InputObject)]
pub struct BeginSecurityKeyRegistrationInput {
    /// Required once you have a key: an assertion over a `beginSecurityKeyChange` ceremony.
    pub authorization: Option<AssertionInput>,
}

#[derive(InputObject)]
pub struct FinishSecurityKeyRegistrationInput {
    pub ceremony_qid: ID,
    /// What to call the key, e.g. "YubiKey on my keychain".
    pub name: String,
    /// `response.clientDataJSON`, base64url.
    pub client_data_json: String,
    /// `response.attestationObject`, base64url.
    pub attestation_object: String,
}

#[derive(InputObject)]
pub struct RemoveSecurityKeyInput {
    pub security_key_qid: ID,
    /// An assertion over a `beginSecurityKeyChange` ceremony, from any of your keys.
    pub authorization: AssertionInput,
}

#[derive(InputObject)]
pub struct ResetSecurityKeyInput {
    pub security_key_qid: ID,
}

#[derive(InputObject)]
pub struct BeginChallengeVerificationInput {
    pub challenge_qid: ID,
}

#[derive(InputObject)]
pub struct FinishChallengeVerificationInput {
    pub assertion: AssertionInput,
}

#[derive(SimpleObject)]
pub struct SecurityKeyPayload {
    pub security_key: SecurityKey,
}

fn ceremony(ceremony_qid: String, options_json: String) -> WebAuthnCeremony {
    WebAuthnCeremony { ceremony_qid: ID(ceremony_qid), options_json }
}

fn key(k: Option<WireKey>) -> Result<SecurityKeyPayload> {
    Ok(SecurityKeyPayload { security_key: SecurityKey(k.ok_or("awzy-challenge returned no key")?) })
}

#[derive(Default)]
pub struct SecurityKeyQuery;

#[Object]
impl SecurityKeyQuery {
    /// The signed-in person's security keys.
    async fn my_security_keys(&self, ctx: &Context<'_>) -> Result<Vec<SecurityKey>> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let person_id = answering_person(&viewer)?;
        let keys = client(ctx)?.list_security_keys(ListSecurityKeysRequest { person_id }).await.map_err(grpc)?;
        Ok(keys.into_inner().keys.into_iter().map(SecurityKey).collect())
    }

    /// Someone's security keys, to reset a lost one. For those who administer challenges.
    async fn security_keys(&self, ctx: &Context<'_>, person_id: String) -> Result<Vec<SecurityKey>> {
        require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let keys = client(ctx)?.list_security_keys(ListSecurityKeysRequest { person_id }).await.map_err(grpc)?;
        Ok(keys.into_inner().keys.into_iter().map(SecurityKey).collect())
    }
}

#[derive(Default)]
pub struct SecurityKeyMutation;

#[Object]
impl SecurityKeyMutation {
    /// For someone who already has a key: a ceremony whose assertion authorizes adding or
    /// removing one.
    async fn begin_security_key_change(&self, ctx: &Context<'_>) -> Result<WebAuthnCeremony> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = BeginKeyChangeRequest { person_id: answering_person(&viewer)? };
        let res = client(ctx)?.begin_key_change(req).await.map_err(grpc)?.into_inner();
        Ok(ceremony(res.ceremony_qid, res.options_json))
    }

    /// A registration ceremony. Your first key needs nothing else; a later one needs
    /// `authorization` from a key you have.
    async fn begin_security_key_registration(
        &self,
        ctx: &Context<'_>,
        input: BeginSecurityKeyRegistrationInput,
    ) -> Result<WebAuthnCeremony> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = BeginKeyRegistrationRequest {
            person_id: answering_person(&viewer)?,
            authorization: input.authorization.map(AssertionInput::wire).transpose()?,
        };
        let res = client(ctx)?.begin_key_registration(req).await.map_err(grpc)?.into_inner();
        Ok(ceremony(res.ceremony_qid, res.options_json))
    }

    async fn finish_security_key_registration(
        &self,
        ctx: &Context<'_>,
        input: FinishSecurityKeyRegistrationInput,
    ) -> Result<SecurityKeyPayload> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = FinishKeyRegistrationRequest {
            ceremony_qid: input.ceremony_qid.0,
            person_id: answering_person(&viewer)?,
            name: input.name,
            client_data_json: bytes("clientDataJson", &input.client_data_json)?,
            attestation_object: bytes("attestationObject", &input.attestation_object)?,
        };
        key(client(ctx)?.finish_key_registration(req).await.map_err(grpc)?.into_inner().key)
    }

    async fn remove_security_key(
        &self,
        ctx: &Context<'_>,
        input: RemoveSecurityKeyInput,
    ) -> Result<SecurityKeyPayload> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = RemoveSecurityKeyRequest {
            security_key_qid: input.security_key_qid.0,
            person_id: answering_person(&viewer)?,
            authorization: Some(input.authorization.wire()?),
        };
        key(client(ctx)?.remove_security_key(req).await.map_err(grpc)?.into_inner().key)
    }

    /// Removes someone's lost key, so they can enroll a first key again. For those who
    /// administer challenges.
    async fn reset_security_key(&self, ctx: &Context<'_>, input: ResetSecurityKeyInput) -> Result<SecurityKeyPayload> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Challenges).await?;
        let req = ResetSecurityKeyRequest {
            security_key_qid: input.security_key_qid.0,
            removed_by_actor: Some(viewer.actor().into()),
            ..Default::default()
        };
        key(client(ctx)?.reset_security_key(req).await.map_err(grpc)?.into_inner().key)
    }

    /// An assertion ceremony for a verification challenge sent to you.
    async fn begin_challenge_verification(
        &self,
        ctx: &Context<'_>,
        input: BeginChallengeVerificationInput,
    ) -> Result<WebAuthnCeremony> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req =
            BeginVerificationRequest { challenge_qid: input.challenge_qid.0, person_id: answering_person(&viewer)? };
        let res = client(ctx)?.begin_verification(req).await.map_err(grpc)?.into_inner();
        Ok(ceremony(res.ceremony_qid, res.options_json))
    }

    /// Passes the verification with your key's assertion.
    async fn finish_challenge_verification(
        &self,
        ctx: &Context<'_>,
        input: FinishChallengeVerificationInput,
    ) -> Result<ChallengePayload> {
        let viewer = require(ctx, PortalAction::View, PortalArea::Challenges).await?;
        let req = FinishVerificationRequest {
            person_id: answering_person(&viewer)?,
            assertion: Some(input.assertion.wire()?),
        };
        let c = client(ctx)?.finish_verification(req).await.map_err(grpc)?.into_inner().challenge;
        Ok(ChallengePayload { challenge: Challenge(c.ok_or("awzy-challenge returned no challenge")?) })
    }
}
