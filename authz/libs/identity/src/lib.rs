//! Who is calling: validated from a bearer token, never from headers or tool arguments (ADR 0008).
//! Shared by awzy-gateway and the client portal.
//!
//! - No `act` claim, `sub` not a registered service account: a person (`Human`), with groups
//!   from the `groups` claim.
//! - No `act` claim, `sub` a registered service account: it acts on its own authority
//!   (client-credentials token).
//! - An `act` claim (RFC 8693 token exchange): `act.sub` is a service account acting for the
//!   person in `sub` (ADR 0016).

pub mod jwks;

use awzy_model::{entity_type, EntityRef};
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
    /// awzy-context (ADR 0031). Default `sub`; with Okta or Entra usually `email` or
    /// `preferred_username`, since their `sub` is an opaque ID.
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

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
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

#[derive(Debug, Deserialize)]
struct Actor {
    sub: String,
}

/// The validated caller, as the PDP needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Accessor {
    pub subject: EntityRef,
    pub on_behalf_of: Option<EntityRef>,
    pub human_present: Option<bool>,
    /// The person's display name, when the token has one.
    pub name: Option<String>,
    /// Cedar entities for the subject and the person, with their groups and attributes.
    pub entities: Vec<Value>,
}

impl Accessor {
    /// A person acting for themselves, in `groups`.
    pub fn human(id: &str, name: Option<String>, groups: &[String]) -> Self {
        Accessor {
            subject: EntityRef::new(entity_type::HUMAN, id),
            on_behalf_of: None,
            human_present: None,
            name,
            entities: vec![entity(entity_type::HUMAN, id, json!({}), groups)],
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

pub struct Verifier {
    keys: Keys,
    issuer: String,
    audience: String,
    subject_claim: Option<String>,
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
        Ok(Self { keys, issuer: config.issuer.clone(), audience: config.audience.clone(), subject_claim })
    }

    fn validation(&self, alg: Algorithm) -> Validation {
        let mut validation = Validation::new(alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation
    }

    pub async fn accessor<R: ServiceAccounts + ?Sized>(
        &self,
        authorization: Option<&str>,
        registry: &R,
    ) -> Result<Accessor, IdentityError> {
        let token = authorization.and_then(|h| h.strip_prefix("Bearer ")).ok_or(IdentityError::Missing)?;
        let invalid = |e: jsonwebtoken::errors::Error| IdentityError::Invalid(e.to_string());
        let claims = match &self.keys {
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
        };
        let mut claims = claims;
        if let Some(claim) = &self.subject_claim {
            match claims.other.get(claim).and_then(Value::as_str) {
                Some(subject) => claims.sub = subject.to_string(),
                None => {
                    return Err(IdentityError::Invalid(format!(
                        "the token has no {claim:?} claim to identify the person"
                    )))
                }
            }
        }
        accessor_from_claims(claims, registry)
    }
}

fn accessor_from_claims<R: ServiceAccounts + ?Sized>(claims: Claims, registry: &R) -> Result<Accessor, IdentityError> {
    let human = |id: &str, groups: &[String]| entity(entity_type::HUMAN, id, json!({}), groups);
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
        Some(actor) => Accessor {
            subject: EntityRef::new(entity_type::SERVICE_ACCOUNT, &actor.sub),
            on_behalf_of: Some(EntityRef::new(entity_type::HUMAN, &claims.sub)),
            human_present: claims.awzy_human_present,
            name: claims.name,
            entities: vec![service_account(&actor.sub)?, human(&claims.sub, &claims.groups)],
        },
        None if registry.service_account(&claims.sub).is_some()
            || (registry.defers_to_pdp() && is_client_credentials(&claims)) =>
        {
            Accessor {
                subject: EntityRef::new(entity_type::SERVICE_ACCOUNT, &claims.sub),
                on_behalf_of: None,
                human_present: claims.awzy_human_present,
                name: claims.name,
                entities: vec![service_account(&claims.sub)?],
            }
        }
        None => Accessor {
            subject: EntityRef::new(entity_type::HUMAN, &claims.sub),
            on_behalf_of: None,
            human_present: claims.awzy_human_present,
            name: claims.name,
            entities: vec![human(&claims.sub, &claims.groups)],
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
        let a = verifier.accessor(Some(&bearer(alice)), &registry).await.unwrap();
        assert_eq!(
            (a.subject.kind.as_str(), a.subject.id.as_str(), a.actor()),
            ("Human", "alice", awzy_model::Actor::human("alice"))
        );
        assert_eq!(a.name.as_deref(), Some("Alice"));
        assert_eq!(a.entities[0]["parents"][0]["id"], "security");

        let delegated = token(&key, "k1", claims("alice", json!({ "act": { "sub": "sales-genie" } })));
        let d = verifier.accessor(Some(&bearer(delegated)), &registry).await.unwrap();
        assert_eq!(d.subject, EntityRef::new("ServiceAccount", "sales-genie"));
        assert_eq!(d.actor().to_string(), "ServiceAccount::sales-genie for Human::alice");

        let wrong_kid = token(&key, "k2", claims("alice", json!({})));
        assert!(matches!(verifier.accessor(Some(&bearer(wrong_kid)), &registry).await, Err(IdentityError::Invalid(_))));
        let wrong_aud = token(
            &key,
            "k1",
            json!({ "iss": "https://idp.test", "aud": "other", "sub": "alice", "exp": 4_000_000_000_u64 }),
        );
        assert!(matches!(verifier.accessor(Some(&bearer(wrong_aud)), &registry).await, Err(IdentityError::Invalid(_))));
        assert!(matches!(verifier.accessor(None, &registry).await, Err(IdentityError::Missing)));

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
        let a = by_email.accessor(Some(&bearer(okta)), &registry).await.unwrap();
        assert_eq!(a.subject, EntityRef::new("Human", "alice@acme.com"));
        let no_email = token(&key, "k1", claims("00u1abc", json!({})));
        assert!(matches!(by_email.accessor(Some(&bearer(no_email)), &registry).await, Err(IdentityError::Invalid(_))));

        // With the registry deferred to the PDP: accounts pass through as bare entities, and an
        // application's own token is recognized as a service account, not a person.
        let delegated = token(&key, "k1", claims("alice", json!({ "act": { "sub": "anything-bot" } })));
        let d = verifier.accessor(Some(&bearer(delegated)), &DeferToPdp).await.unwrap();
        assert_eq!(d.subject, EntityRef::new("ServiceAccount", "anything-bot"));
        assert_eq!(d.entities[0]["attrs"], json!({}), "the PDP fills it in from awzy-context, or denies it");
        let app = token(&key, "k1", claims("nightly-sync", json!({ "client_id": "nightly-sync" })));
        assert_eq!(
            verifier.accessor(Some(&bearer(app)), &DeferToPdp).await.unwrap().subject,
            EntityRef::new("ServiceAccount", "nightly-sync")
        );
        let entra_app = token(&key, "k1", claims("5f1c-app", json!({ "idtyp": "app" })));
        assert_eq!(
            verifier.accessor(Some(&bearer(entra_app)), &DeferToPdp).await.unwrap().subject.kind,
            "ServiceAccount"
        );
        let person = token(&key, "k1", claims("bob", json!({ "client_id": "portal" })));
        assert_eq!(
            verifier.accessor(Some(&bearer(person)), &DeferToPdp).await.unwrap().subject,
            EntityRef::new("Human", "bob")
        );
    }
}
