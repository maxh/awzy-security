//! Who is calling: validated from a bearer token, never from headers or tool arguments (ADR 0008).
//! Shared by awzy-gateway and the client portal.
//!
//! - No `act` claim, `sub` not a registered service account: a person (`Human`), with groups
//!   from the `groups` claim.
//! - No `act` claim, `sub` a registered service account: it acts on its own authority
//!   (client-credentials token).
//! - An `act` claim (RFC 8693 token exchange): `act.sub` is a service account acting for the
//!   person in `sub` (ADR 0016).
//!
//! Besides the client's IdP, apps an admin registered sign tokens for their own agents, the
//! service accounts under their prefix ([`TrustedIssuers`]; ADR 0110).

pub mod jwks;
mod verified;

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use awzy_model::{entity_type, EntityRef, TrustedIssuer};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::{json, Value};

pub use jwks::Jwks;

#[derive(Debug, Clone, Deserialize)]
pub struct TokenConfig {
    pub issuer: String,
    pub audience: String,
    /// PEM certificates to trust for the JWKS endpoint besides the public roots, for an IdP
    /// behind a private CA.
    #[serde(default)]
    pub jwks_ca_file: Option<String>,
    /// The claim that identifies a person: their Human ID, matched against SCIM `userName` in
    /// awzy-context (ADR 0031). Default `sub`; with Okta or Entra usually `email`, since their
    /// `sub` is an opaque ID. Should be one people can't change themselves (Entra's
    /// `preferred_username` can be). Tokens an application gets for itself keep `sub`.
    #[serde(default)]
    pub subject_claim: Option<String>,
    #[serde(flatten)]
    pub key: TokenKey,
}

/// Where the keys that sign tokens come from.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKey {
    /// The IdP's JWKS endpoint (RS256 or ES256 keys), refreshed when a token names a new key.
    /// HTTPS, or plain HTTP to a loopback address for local development.
    JwksUrl(String),
    /// A fixed RSA public key (PEM).
    Rs256PublicKeyFile(String),
    /// Shared secret from the named environment variable. Development only.
    Hs256SecretEnv(String),
}

/// A service account the install knows: its owner, whether it's AI, and its groups.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceAccountConfig {
    pub id: String,
    /// The accountable person's ID.
    pub owner: String,
    #[serde(default)]
    pub ai: bool,
    #[serde(default)]
    pub groups: Vec<String>,
}

/// Looks up registered service accounts: from config, or deferred to the PDP, which takes them
/// from awzy-context (ADR 0033).
pub trait ServiceAccounts: Send + Sync {
    fn service_account(&self, id: &str) -> Option<&ServiceAccountConfig>;

    /// True when the PDP decides registration: an account not known here is passed on as a bare
    /// entity, which the PDP replaces from awzy-context or denies as unregistered.
    fn defers_to_pdp(&self) -> bool {
        false
    }
}

/// The registry is awzy-context's, through the PDP.
pub struct DeferToPdp;

impl ServiceAccounts for DeferToPdp {
    fn service_account(&self, _: &str) -> Option<&ServiceAccountConfig> {
        None
    }

    fn defers_to_pdp(&self) -> bool {
        true
    }
}

impl ServiceAccounts for [ServiceAccountConfig] {
    fn service_account(&self, id: &str) -> Option<&ServiceAccountConfig> {
        self.iter().find(|s| s.id == id)
    }
}

impl ServiceAccounts for Vec<ServiceAccountConfig> {
    fn service_account(&self, id: &str) -> Option<&ServiceAccountConfig> {
        self.as_slice().service_account(id)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("missing bearer token")]
    Missing,
    #[error("invalid token: {0}")]
    Invalid(String),
    #[error("service account {0:?} is not registered")]
    UnregisteredServiceAccount(String),
}

#[derive(Debug, Clone, Deserialize)]
struct Claims {
    sub: String,
    /// Seconds; some IdPs write it with a fraction.
    exp: f64,
    #[serde(default)]
    act: Option<Actor>,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    name: Option<String>,
    /// Whether a person is at the keyboard to answer a challenge. Unset: the PDP's default.
    #[serde(default)]
    awzy_human_present: Option<bool>,
    /// Everything else, for a configured subject claim.
    #[serde(flatten)]
    other: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct Actor {
    sub: String,
}

/// The validated caller, as the PDP needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    pub subject: EntityRef,
    pub on_behalf_of: Option<EntityRef>,
    pub human_present: Option<bool>,
    /// The person's display name, when the token has one.
    pub name: Option<String>,
    /// Cedar entities for the subject and the person, with their groups and attributes.
    pub entities: Vec<Value>,
    /// When the token stops being valid, so a long-lived connection signed in with it can end
    /// then. None for a principal not made from a token.
    pub expires_at: Option<std::time::SystemTime>,
}

impl Principal {
    /// A person acting for themselves, in `groups`.
    pub fn human(id: &str, name: Option<String>, groups: &[String]) -> Self {
        Principal {
            subject: EntityRef::new(entity_type::HUMAN, id),
            on_behalf_of: None,
            human_present: None,
            name,
            entities: vec![entity(entity_type::HUMAN, id, json!({}), groups)],
            expires_at: None,
        }
    }

    /// Who to attribute an action to (ADR 0060): `Human::alice`, or
    /// `ServiceAccount::sales-genie for Human::alice`.
    pub fn actor(&self) -> awzy_model::Actor {
        awzy_model::Actor::new(self.subject.clone(), self.on_behalf_of.clone())
    }
}

enum Keys {
    Static(DecodingKey, Algorithm),
    Jwks(Box<Jwks>),
}

/// Apps trusted to sign tokens for their own agents (ADR 0110), by the `iss` their tokens carry.
/// A handle: the verifier and whatever refreshes the list from the PDP share it.
/// How often enforcement points ask the PDP for the list: a removed app's tokens stop within it.
const REFRESH: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Clone, Default)]
pub struct TrustedIssuers {
    apps: Arc<RwLock<HashMap<String, AppIssuer>>>,
    /// The IdP's private CA, trusted for apps' JWKS endpoints too.
    ca: Option<Arc<[u8]>>,
}

#[derive(Clone)]
struct AppIssuer {
    config: TrustedIssuer,
    jwks: Arc<Jwks>,
}

impl TrustedIssuers {
    /// Replaces the list. An app whose JWKS URL is unchanged keeps the keys it has fetched.
    pub fn set(&self, issuers: Vec<TrustedIssuer>) {
        let mut apps = self.apps.write().unwrap_or_else(|e| e.into_inner());
        let mut next = HashMap::new();
        for config in issuers {
            let jwks = match apps.get(&config.issuer).filter(|a| a.config.jwks_url == config.jwks_url) {
                Some(app) => app.jwks.clone(),
                None => match Jwks::new(&config.jwks_url, self.ca.as_deref()) {
                    Ok(jwks) => Arc::new(jwks),
                    Err(e) => {
                        tracing::warn!(app = %config.name, error = %e, "can't use a trusted issuer's JWKS URL");
                        continue;
                    }
                },
            };
            next.insert(config.issuer.clone(), AppIssuer { config, jwks });
        }
        *apps = next;
    }

    /// Keeps each of `lists` current from `fetch` (the PDP's `ListTrustedIssuers`), forever. A
    /// failed fetch keeps the lists in force.
    pub async fn follow<F, Fut>(lists: Vec<TrustedIssuers>, mut fetch: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<Vec<TrustedIssuer>>>,
    {
        let mut every = tokio::time::interval(REFRESH);
        loop {
            every.tick().await;
            match fetch().await {
                Ok(issuers) => lists.iter().for_each(|l| l.set(issuers.clone())),
                Err(e) => {
                    tracing::warn!(error = %e, "couldn't refresh the trusted token issuers; keeping the last list")
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.apps.read().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    fn get(&self, iss: &str) -> Option<AppIssuer> {
        self.apps.read().unwrap_or_else(|e| e.into_inner()).get(iss).cloned()
    }
}

pub struct Verifier {
    keys: Keys,
    issuer: String,
    audience: String,
    subject_claim: Option<String>,
    verified: verified::Verified,
    apps: TrustedIssuers,
}

impl Verifier {
    pub fn new(config: &TokenConfig) -> anyhow::Result<Self> {
        let keys = match &config.key {
            TokenKey::Hs256SecretEnv(var) => {
                let secret = std::env::var(var).map_err(|_| anyhow::anyhow!("{var} is not set"))?;
                Keys::Static(DecodingKey::from_secret(secret.as_bytes()), Algorithm::HS256)
            }
            TokenKey::Rs256PublicKeyFile(path) => {
                Keys::Static(DecodingKey::from_rsa_pem(&std::fs::read(path)?)?, Algorithm::RS256)
            }
            TokenKey::JwksUrl(url) => {
                let ca = config.jwks_ca_file.as_ref().map(std::fs::read).transpose()?;
                Keys::Jwks(Box::new(Jwks::new(url, ca.as_deref())?))
            }
        };
        let subject_claim = config.subject_claim.clone().filter(|c| c != "sub");
        let ca = config.jwks_ca_file.as_ref().map(std::fs::read).transpose()?.map(Arc::from);
        Ok(Self {
            keys,
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            subject_claim,
            verified: Default::default(),
            apps: TrustedIssuers { apps: Arc::default(), ca },
        })
    }

    /// The apps this verifier trusts besides the IdP, to keep up to date from the PDP.
    pub fn trusted_issuers(&self) -> TrustedIssuers {
        self.apps.clone()
    }

    /// The app that signed this token, when one did rather than the IdP. Read before the
    /// signature is checked, only to choose the keys to check it with.
    fn app_for(&self, token: &str) -> Option<AppIssuer> {
        if self.apps.is_empty() {
            return None;
        }
        #[derive(Deserialize)]
        struct Iss {
            iss: String,
        }
        let payload = URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
        let iss = serde_json::from_slice::<Iss>(&payload).ok()?.iss;
        (iss != self.issuer).then(|| self.apps.get(&iss)).flatten()
    }

    /// A token an app signed, checked against its keys, `iss`, and `aud`.
    async fn app_claims(&self, token: &str, app: &AppIssuer) -> Result<Claims, IdentityError> {
        let invalid = |e: jsonwebtoken::errors::Error| IdentityError::Invalid(e.to_string());
        let header = decode_header(token).map_err(invalid)?;
        if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
            return Err(IdentityError::Invalid(format!("algorithm {:?} is not accepted", header.alg)));
        }
        let kid = header.kid.ok_or_else(|| IdentityError::Invalid("token has no key ID (kid)".into()))?;
        let key = app.jwks.key(&kid).await.map_err(|e| IdentityError::Invalid(e.to_string()))?;
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&app.config.issuer]);
        validation.set_audience(&[&app.config.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        Ok(decode::<Claims>(token, &key, &validation).map_err(invalid)?.claims)
    }

    /// The app a token it signed for itself names (`sub` is its name), for registering and
    /// retiring its agents. Any other token is refused.
    pub async fn app_token(&self, authorization: Option<&str>) -> Result<String, IdentityError> {
        let token = authorization.and_then(|h| h.strip_prefix("Bearer ")).ok_or(IdentityError::Missing)?;
        let app = self.app_for(token).ok_or_else(|| IdentityError::Invalid("not signed by a trusted app".into()))?;
        let claims = self.app_claims(token, &app).await?;
        if claims.act.is_some() || claims.sub != app.config.name {
            return Err(IdentityError::Invalid(format!("not {}'s own token", app.config.name)));
        }
        Ok(app.config.name)
    }

    fn validation(&self, alg: Algorithm) -> Validation {
        let mut validation = Validation::new(alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation
    }

    pub async fn principal<R: ServiceAccounts + ?Sized>(
        &self,
        authorization: Option<&str>,
        registry: &R,
    ) -> Result<Principal, IdentityError> {
        let token = authorization.and_then(|h| h.strip_prefix("Bearer ")).ok_or(IdentityError::Missing)?;
        // The cache holds only the IdP's tokens, so a hit skips looking for an app.
        let mut claims = match self.verified.get(token) {
            Some(claims) => claims,
            None => {
                if let Some(app) = self.app_for(token) {
                    return app_principal(self.app_claims(token, &app).await?, &app.config, registry);
                }
                let claims = self.verify(token).await?;
                self.verified.insert(token, &claims);
                claims
            }
        };
        // The configured claim names a person. An application's own token has none and keeps its
        // `sub`, which is how it's registered.
        let acts_for_itself = claims.act.is_none()
            && (registry.service_account(&claims.sub).is_some()
                || (registry.defers_to_pdp() && is_client_credentials(&claims)));
        if let Some(claim) = self.subject_claim.as_ref().filter(|_| !acts_for_itself) {
            match claims.other.get(claim).and_then(Value::as_str) {
                Some(subject) => claims.sub = subject.to_string(),
                None => {
                    return Err(IdentityError::Invalid(format!(
                        "the token has no {claim:?} claim to identify the person"
                    )))
                }
            }
        }
        principal_from_claims(claims, registry, acts_for_itself)
    }

    /// The token's claims, once its signature, issuer, audience, and expiry check out.
    async fn verify(&self, token: &str) -> Result<Claims, IdentityError> {
        let invalid = |e: jsonwebtoken::errors::Error| IdentityError::Invalid(e.to_string());
        Ok(match &self.keys {
            Keys::Static(key, alg) => decode::<Claims>(token, key, &self.validation(*alg)).map_err(invalid)?.claims,
            Keys::Jwks(jwks) => {
                let header = decode_header(token).map_err(invalid)?;
                if !matches!(header.alg, Algorithm::RS256 | Algorithm::ES256) {
                    return Err(IdentityError::Invalid(format!("algorithm {:?} is not accepted", header.alg)));
                }
                let kid = header.kid.ok_or_else(|| IdentityError::Invalid("token has no key ID (kid)".into()))?;
                let key = jwks.key(&kid).await.map_err(|e| IdentityError::Invalid(e.to_string()))?;
                decode::<Claims>(token, &key, &self.validation(header.alg)).map_err(invalid)?.claims
            }
        })
    }
}

/// An app may sign only for its own agents: a service account under its prefix, on its own or
/// acting for a person, and never a person on their own.
fn app_principal<R: ServiceAccounts + ?Sized>(
    claims: Claims,
    app: &TrustedIssuer,
    registry: &R,
) -> Result<Principal, IdentityError> {
    let agent = claims.act.as_ref().map_or(&claims.sub, |a| &a.sub);
    if claims.act.is_none() && claims.sub == app.name {
        return Err(IdentityError::Invalid(format!("{}'s own token is only for registering its agents", app.name)));
    }
    if !app.may_sign_for(agent) {
        return Err(IdentityError::Invalid(format!(
            "{} signs tokens only for service accounts under {:?}, not {agent:?}",
            app.name,
            app.agent_prefix()
        )));
    }
    let acts_for_itself = claims.act.is_none();
    principal_from_claims(claims, registry, acts_for_itself)
}

/// `acts_for_itself`: a service account's own token, with no `act` (client credentials, or an
/// app's agent).
fn principal_from_claims<R: ServiceAccounts + ?Sized>(
    claims: Claims,
    registry: &R,
    acts_for_itself: bool,
) -> Result<Principal, IdentityError> {
    let human = |id: &str, groups: &[String]| entity(entity_type::HUMAN, id, json!({}), groups);
    let expires_at =
        std::time::Duration::try_from_secs_f64(claims.exp).ok().and_then(|exp| std::time::UNIX_EPOCH.checked_add(exp));
    let service_account = |id: &str| {
        if registry.defers_to_pdp() {
            return Ok(entity(entity_type::SERVICE_ACCOUNT, id, json!({}), &[]));
        }
        let sa =
            registry.service_account(id).ok_or_else(|| IdentityError::UnregisteredServiceAccount(id.to_string()))?;
        let attrs = json!({ "owner": { "__entity": { "type": entity_type::HUMAN, "id": sa.owner } }, "ai": sa.ai });
        Ok::<_, IdentityError>(entity(entity_type::SERVICE_ACCOUNT, &sa.id, attrs, &sa.groups))
    };

    Ok(match claims.act {
        Some(actor) => Principal {
            subject: EntityRef::new(entity_type::SERVICE_ACCOUNT, &actor.sub),
            on_behalf_of: Some(EntityRef::new(entity_type::HUMAN, &claims.sub)),
            human_present: claims.awzy_human_present,
            name: claims.name,
            entities: vec![service_account(&actor.sub)?, human(&claims.sub, &claims.groups)],
            expires_at,
        },
        None if acts_for_itself => Principal {
            subject: EntityRef::new(entity_type::SERVICE_ACCOUNT, &claims.sub),
            on_behalf_of: None,
            human_present: claims.awzy_human_present,
            name: claims.name,
            entities: vec![service_account(&claims.sub)?],
            expires_at,
        },
        None => Principal {
            subject: EntityRef::new(entity_type::HUMAN, &claims.sub),
            on_behalf_of: None,
            human_present: claims.awzy_human_present,
            name: claims.name,
            entities: vec![human(&claims.sub, &claims.groups)],
            expires_at,
        },
    })
}

/// A token an application got for itself (client credentials), not for a person: RFC 9068's
/// `client_id` equal to `sub`, Okta's `cid`, or Entra's `idtyp: app`.
fn is_client_credentials(claims: &Claims) -> bool {
    let claim = |k: &str| claims.other.get(k).and_then(Value::as_str);
    claim("client_id") == Some(claims.sub.as_str())
        || claim("cid") == Some(claims.sub.as_str())
        || claim("idtyp") == Some("app")
}

fn entity(kind: &str, id: &str, attrs: Value, groups: &[String]) -> Value {
    let parents: Vec<Value> = groups.iter().map(|g| json!({ "type": entity_type::GROUP, "id": g })).collect();
    json!({ "uid": { "type": kind, "id": id }, "attrs": attrs, "parents": parents })
}

#[cfg(test)]
mod tests {
    use axum::{routing::get, Json, Router};
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use p256::{
        ecdsa::SigningKey,
        pkcs8::{EncodePrivateKey, LineEnding},
    };

    use super::*;

    /// An ES256 key, and an HTTPS JWKS endpoint serving its JWK with the CA file to trust.
    async fn idp(kid: &str) -> (EncodingKey, String, String) {
        let key = SigningKey::from_bytes(&[21u8; 32].into()).unwrap();
        let point = key.verifying_key().to_encoded_point(false);
        let jwk = json!({
            "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": kid,
            "x": URL_SAFE_NO_PAD.encode(point.x().unwrap()),
            "y": URL_SAFE_NO_PAD.encode(point.y().unwrap()),
        });
        let app = Router::new().route("/jwks", get(move || async move { Json(json!({ "keys": [jwk] })) }));
        let (base, ca) = crate::jwks::tests::serve_https(app).await;
        let ca_file = std::env::temp_dir().join(format!("awzy-identity-test-ca-{}.pem", std::process::id()));
        std::fs::write(&ca_file, ca).unwrap();
        let pem = key.to_pkcs8_pem(LineEnding::LF).unwrap();
        (EncodingKey::from_ec_pem(pem.as_bytes()).unwrap(), format!("{base}/jwks"), ca_file.display().to_string())
    }

    fn token(key: &EncodingKey, kid: &str, claims: Value) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(kid.into());
        encode(&header, &claims, key).unwrap()
    }

    fn claims(sub: &str, extra: Value) -> Value {
        let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 300;
        let mut c = json!({ "iss": "https://idp.test", "aud": "awzy", "sub": sub, "exp": exp });
        c.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        c
    }

    #[tokio::test]
    async fn validates_es256_tokens_from_jwks() {
        let (key, url, ca_file) = idp("k1").await;
        let verifier = Verifier::new(&TokenConfig {
            issuer: "https://idp.test".into(),
            audience: "awzy".into(),
            jwks_ca_file: Some(ca_file.clone()),
            subject_claim: None,
            key: TokenKey::JwksUrl(url.clone()),
        })
        .unwrap();
        let registry =
            vec![ServiceAccountConfig { id: "sales-genie".into(), owner: "bob".into(), ai: true, groups: vec![] }];
        let bearer = |t: String| format!("Bearer {t}");

        let alice = token(&key, "k1", claims("alice", json!({ "groups": ["security"], "name": "Alice" })));
        let a = verifier.principal(Some(&bearer(alice)), &registry).await.unwrap();
        assert_eq!(
            (a.subject.kind.as_str(), a.subject.id.as_str(), a.actor()),
            ("Human", "alice", awzy_model::Actor::human("alice"))
        );
        assert_eq!(a.name.as_deref(), Some("Alice"));
        assert!(a.expires_at.unwrap() > std::time::SystemTime::now(), "the token's exp comes along");
        assert_eq!(a.entities[0]["parents"][0]["id"], "security");

        let delegated = token(&key, "k1", claims("alice", json!({ "act": { "sub": "sales-genie" } })));
        let d = verifier.principal(Some(&bearer(delegated)), &registry).await.unwrap();
        assert_eq!(d.subject, EntityRef::new("ServiceAccount", "sales-genie"));
        assert_eq!(d.actor().to_string(), "ServiceAccount::sales-genie for Human::alice");

        let wrong_kid = token(&key, "k2", claims("alice", json!({})));
        assert!(matches!(
            verifier.principal(Some(&bearer(wrong_kid)), &registry).await,
            Err(IdentityError::Invalid(_))
        ));
        let wrong_aud = token(
            &key,
            "k1",
            json!({ "iss": "https://idp.test", "aud": "other", "sub": "alice", "exp": 4_000_000_000_u64 }),
        );
        assert!(matches!(
            verifier.principal(Some(&bearer(wrong_aud)), &registry).await,
            Err(IdentityError::Invalid(_))
        ));
        assert!(matches!(verifier.principal(None, &registry).await, Err(IdentityError::Missing)));

        // Okta-style: an opaque sub, the person identified by email as awzy-context knows them.
        let by_email = Verifier::new(&TokenConfig {
            issuer: "https://idp.test".into(),
            audience: "awzy".into(),
            jwks_ca_file: Some(ca_file),
            subject_claim: Some("email".into()),
            key: TokenKey::JwksUrl(url),
        })
        .unwrap();
        let okta = token(&key, "k1", claims("00u1abc", json!({ "email": "alice@acme.com" })));
        let a = by_email.principal(Some(&bearer(okta)), &registry).await.unwrap();
        assert_eq!(a.subject, EntityRef::new("Human", "alice@acme.com"));
        let no_email = token(&key, "k1", claims("00u1abc", json!({})));
        assert!(matches!(by_email.principal(Some(&bearer(no_email)), &registry).await, Err(IdentityError::Invalid(_))));
        // An application's own token has no email, and is known by its sub.
        let registered = token(&key, "k1", claims("sales-genie", json!({})));
        assert_eq!(
            by_email.principal(Some(&bearer(registered)), &registry).await.unwrap().subject,
            EntityRef::new("ServiceAccount", "sales-genie")
        );
        let okta_app = token(&key, "k1", claims("0oa9app", json!({ "cid": "0oa9app" })));
        assert_eq!(
            by_email.principal(Some(&bearer(okta_app)), &DeferToPdp).await.unwrap().subject,
            EntityRef::new("ServiceAccount", "0oa9app")
        );

        // With the registry deferred to the PDP: accounts pass through as bare entities, and an
        // application's own token is recognized as a service account, not a person.
        let delegated = token(&key, "k1", claims("alice", json!({ "act": { "sub": "anything-bot" } })));
        let d = verifier.principal(Some(&bearer(delegated)), &DeferToPdp).await.unwrap();
        assert_eq!(d.subject, EntityRef::new("ServiceAccount", "anything-bot"));
        assert_eq!(d.entities[0]["attrs"], json!({}), "the PDP fills it in from awzy-context, or denies it");
        let app = token(&key, "k1", claims("nightly-sync", json!({ "client_id": "nightly-sync" })));
        assert_eq!(
            verifier.principal(Some(&bearer(app)), &DeferToPdp).await.unwrap().subject,
            EntityRef::new("ServiceAccount", "nightly-sync")
        );
        let entra_app = token(&key, "k1", claims("5f1c-app", json!({ "idtyp": "app" })));
        assert_eq!(
            verifier.principal(Some(&bearer(entra_app)), &DeferToPdp).await.unwrap().subject.kind,
            "ServiceAccount"
        );
        let human = token(&key, "k1", claims("bob", json!({ "client_id": "portal" })));
        assert_eq!(
            verifier.principal(Some(&bearer(human)), &DeferToPdp).await.unwrap().subject,
            EntityRef::new("Human", "bob")
        );
    }

    #[tokio::test]
    async fn an_app_signs_tokens_only_for_its_own_agents() {
        let (key, url, ca_file) = idp("k1").await;
        let verifier = Verifier::new(&TokenConfig {
            issuer: "https://idp.test".into(),
            audience: "awzy".into(),
            jwks_ca_file: Some(ca_file),
            subject_claim: Some("email".into()),
            key: TokenKey::JwksUrl(url.clone()),
        })
        .unwrap();
        let bearer = |t: String| format!("Bearer {t}");
        let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 300;
        let crm = |sub: &str, extra: Value| {
            let mut c = json!({ "iss": "https://crm.acme.example", "aud": "awzy-agents", "sub": sub, "exp": exp });
            c.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            bearer(token(&key, "k1", c))
        };
        let agent = crm("acme-crm.renewals-bot", json!({}));
        assert!(
            matches!(verifier.principal(Some(&agent), &DeferToPdp).await, Err(IdentityError::Invalid(_))),
            "not trusted until an admin registers the app"
        );

        verifier.trusted_issuers().set(vec![TrustedIssuer {
            name: "acme-crm".into(),
            issuer: "https://crm.acme.example".into(),
            jwks_url: url,
            audience: "awzy-agents".into(),
        }]);
        let own = verifier.principal(Some(&agent), &DeferToPdp).await.unwrap();
        assert_eq!(own.subject, EntityRef::new("ServiceAccount", "acme-crm.renewals-bot"));
        assert_eq!(own.on_behalf_of, None);
        let delegated = crm("alice@acme.com", json!({ "act": { "sub": "acme-crm.assistant" } }));
        let d = verifier.principal(Some(&delegated), &DeferToPdp).await.unwrap();
        assert_eq!(d.actor().to_string(), "ServiceAccount::acme-crm.assistant for Human::alice@acme.com");

        for refused in [
            crm("alice@acme.com", json!({})),
            crm("sales-genie", json!({})),
            crm("alice@acme.com", json!({ "act": { "sub": "sales-genie" } })),
            crm("acme-crm", json!({})),
            crm("acme-crm.renewals-bot", json!({ "aud": "awzy" })),
        ] {
            assert!(
                matches!(verifier.principal(Some(&refused), &DeferToPdp).await, Err(IdentityError::Invalid(_))),
                "{refused}"
            );
        }
        let registry: Vec<ServiceAccountConfig> = vec![];
        assert!(matches!(
            verifier.principal(Some(&agent), &registry).await,
            Err(IdentityError::UnregisteredServiceAccount(_))
        ));

        assert_eq!(verifier.app_token(Some(&crm("acme-crm", json!({})))).await.unwrap(), "acme-crm");
        assert!(verifier.app_token(Some(&agent)).await.is_err(), "an agent's token doesn't register agents");
        let idp_token = bearer(token(&key, "k1", claims("acme-crm", json!({}))));
        assert!(verifier.app_token(Some(&idp_token)).await.is_err());

        verifier.trusted_issuers().set(vec![]);
        assert!(verifier.principal(Some(&agent), &DeferToPdp).await.is_err(), "removed: no longer trusted");
    }
}
