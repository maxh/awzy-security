//! WebAuthn relying-party checks for security keys (ADR 0059), narrowed to one profile:
//! ES256 (ECDSA P-256 with SHA-256) credentials, and "none" attestation.
//!
//! awzy asks for no attestation and doesn't check the attestation statement an authenticator
//! sends anyway: a key proves possession of what was enrolled, not who made it. Everything a
//! signature covers is checked: the challenge awzy issued, the origin, the RP ID hash, user
//! presence, and the signature counter. Anything outside the profile is refused rather than
//! half-supported.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ciborium::Value;
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// COSE algorithm -7: ECDSA with SHA-256. The only one accepted.
pub const ES256: i64 = -7;

const FLAG_USER_PRESENT: u8 = 0x01;
const FLAG_USER_VERIFIED: u8 = 0x04;
const FLAG_ATTESTED_CREDENTIAL: u8 = 0x40;

/// Longest credential ID the spec allows.
const MAX_CREDENTIAL_ID: usize = 1023;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WebAuthnError {
    #[error("the authenticator's response is malformed: {0}")]
    Malformed(&'static str),
    #[error("the response is for a different ceremony (expected {expected}, got {got:?})")]
    WrongType { expected: &'static str, got: String },
    #[error("the response answers a different challenge")]
    WrongChallenge,
    #[error("the response came from an origin awzy doesn't serve: {0:?}")]
    WrongOrigin(String),
    #[error("the response was made inside a cross-origin frame")]
    CrossOrigin,
    #[error("the key was registered for a different site")]
    WrongRelyingParty,
    #[error("the security key wasn't touched (no user presence)")]
    NoUserPresence,
    #[error("only ES256 (P-256) security keys are supported")]
    UnsupportedKey,
    #[error("the signature doesn't verify")]
    BadSignature,
    #[error("the security key's counter went backwards; it may have been cloned")]
    CounterRegressed,
}

/// Who awzy is to authenticators: the RP ID (a domain) and the exact origins pages are served
/// from.
#[derive(Debug, Clone)]
pub struct RelyingParty {
    id: String,
    origins: Vec<String>,
}

/// A credential from a successful registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCredential {
    pub credential_id: Vec<u8>,
    /// SEC1 uncompressed point.
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

/// A successful assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Asserted {
    pub sign_count: u32,
    /// Whether the authenticator verified the person (PIN or biometric), not just presence.
    pub user_verified: bool,
}

#[derive(Deserialize)]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
}

/// The fixed-size head of authenticator data.
struct AuthData<'a> {
    rp_id_hash: &'a [u8],
    flags: u8,
    sign_count: u32,
    rest: &'a [u8],
}

fn parse_auth_data(data: &[u8]) -> Result<AuthData<'_>, WebAuthnError> {
    if data.len() < 37 {
        return Err(WebAuthnError::Malformed("authenticator data is too short"));
    }
    Ok(AuthData {
        rp_id_hash: &data[..32],
        flags: data[32],
        sign_count: u32::from_be_bytes(data[33..37].try_into().expect("4 bytes")),
        rest: &data[37..],
    })
}

impl RelyingParty {
    pub fn new(id: impl Into<String>, origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { id: id.into(), origins: origins.into_iter().map(Into::into).collect() }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    fn check_client_data(
        &self,
        json: &[u8],
        expected_type: &'static str,
        challenge: &[u8],
    ) -> Result<(), WebAuthnError> {
        let cd: ClientData = serde_json::from_slice(json)
            .map_err(|_| WebAuthnError::Malformed("client data isn't the expected JSON"))?;
        if cd.kind != expected_type {
            return Err(WebAuthnError::WrongType { expected: expected_type, got: cd.kind });
        }
        let got =
            URL_SAFE_NO_PAD.decode(cd.challenge.trim_end_matches('=')).map_err(|_| WebAuthnError::WrongChallenge)?;
        if got != challenge {
            return Err(WebAuthnError::WrongChallenge);
        }
        if !self.origins.contains(&cd.origin) {
            return Err(WebAuthnError::WrongOrigin(cd.origin));
        }
        if cd.cross_origin {
            return Err(WebAuthnError::CrossOrigin);
        }
        Ok(())
    }

    fn check_auth_data<'a>(&self, data: &'a [u8]) -> Result<AuthData<'a>, WebAuthnError> {
        let ad = parse_auth_data(data)?;
        if ad.rp_id_hash != Sha256::digest(self.id.as_bytes()).as_slice() {
            return Err(WebAuthnError::WrongRelyingParty);
        }
        if ad.flags & FLAG_USER_PRESENT == 0 {
            return Err(WebAuthnError::NoUserPresence);
        }
        Ok(ad)
    }

    /// Checks a `navigator.credentials.create()` response against the challenge awzy issued,
    /// and returns the new credential.
    pub fn verify_registration(
        &self,
        challenge: &[u8],
        client_data_json: &[u8],
        attestation_object: &[u8],
    ) -> Result<NewCredential, WebAuthnError> {
        self.check_client_data(client_data_json, "webauthn.create", challenge)?;
        let object: Value = ciborium::from_reader(attestation_object)
            .map_err(|_| WebAuthnError::Malformed("the attestation object isn't CBOR"))?;
        let auth_data = map_get_text(&object, "authData")
            .and_then(Value::as_bytes)
            .ok_or(WebAuthnError::Malformed("the attestation object has no authData"))?;
        let ad = self.check_auth_data(auth_data)?;
        if ad.flags & FLAG_ATTESTED_CREDENTIAL == 0 {
            return Err(WebAuthnError::Malformed("no credential in the registration"));
        }
        // aaguid (16), credential ID length (2), credential ID, then the COSE key.
        let rest = ad.rest;
        if rest.len() < 18 {
            return Err(WebAuthnError::Malformed("attested credential data is too short"));
        }
        let id_len = u16::from_be_bytes([rest[16], rest[17]]) as usize;
        if id_len == 0 || id_len > MAX_CREDENTIAL_ID || rest.len() < 18 + id_len {
            return Err(WebAuthnError::Malformed("bad credential ID length"));
        }
        let credential_id = rest[18..18 + id_len].to_vec();
        // Extensions may follow the key; reading one CBOR value stops at its end.
        let cose: Value = ciborium::from_reader(&rest[18 + id_len..])
            .map_err(|_| WebAuthnError::Malformed("the credential's public key isn't CBOR"))?;
        let key = es256_key(&cose)?;
        Ok(NewCredential {
            credential_id,
            public_key: key.to_encoded_point(false).as_bytes().to_vec(),
            sign_count: ad.sign_count,
        })
    }

    /// Checks a `navigator.credentials.get()` response from the key with `public_key` (SEC1)
    /// and last-seen counter `sign_count`.
    pub fn verify_assertion(
        &self,
        challenge: &[u8],
        public_key: &[u8],
        sign_count: u32,
        client_data_json: &[u8],
        authenticator_data: &[u8],
        signature: &[u8],
    ) -> Result<Asserted, WebAuthnError> {
        self.check_client_data(client_data_json, "webauthn.get", challenge)?;
        let ad = self.check_auth_data(authenticator_data)?;
        let key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| WebAuthnError::UnsupportedKey)?;
        let sig = Signature::from_der(signature).map_err(|_| WebAuthnError::BadSignature)?;
        // Authenticators may send either S; RustCrypto verifies the low one.
        let sig = sig.normalize_s().unwrap_or(sig);
        let mut signed = authenticator_data.to_vec();
        signed.extend_from_slice(&Sha256::digest(client_data_json));
        key.verify(&signed, &sig).map_err(|_| WebAuthnError::BadSignature)?;
        // Keys that don't count send 0 every time. Any key that does must move forward.
        if (ad.sign_count != 0 || sign_count != 0) && ad.sign_count <= sign_count {
            return Err(WebAuthnError::CounterRegressed);
        }
        Ok(Asserted { sign_count: ad.sign_count, user_verified: ad.flags & FLAG_USER_VERIFIED != 0 })
    }
}

fn map_get_text<'a>(map: &'a Value, key: &str) -> Option<&'a Value> {
    map.as_map()?.iter().find(|(k, _)| k.as_text() == Some(key)).map(|(_, v)| v)
}

fn map_get_int(map: &Value, key: i64) -> Option<&Value> {
    map.as_map()?.iter().find(|(k, _)| k.as_integer().and_then(|i| i64::try_from(i).ok()) == Some(key)).map(|(_, v)| v)
}

fn int(v: Option<&Value>) -> Option<i64> {
    v?.as_integer().and_then(|i| i64::try_from(i).ok())
}

/// A COSE_Key that is EC2, P-256, ES256.
fn es256_key(cose: &Value) -> Result<VerifyingKey, WebAuthnError> {
    // kty 1 = 2 (EC2), alg 3 = -7 (ES256), crv -1 = 1 (P-256), x -2, y -3.
    if int(map_get_int(cose, 1)) != Some(2)
        || int(map_get_int(cose, 3)) != Some(ES256)
        || int(map_get_int(cose, -1)) != Some(1)
    {
        return Err(WebAuthnError::UnsupportedKey);
    }
    let coord = |k| map_get_int(cose, k).and_then(Value::as_bytes).filter(|b| b.len() == 32);
    let (Some(x), Some(y)) = (coord(-2), coord(-3)) else { return Err(WebAuthnError::UnsupportedKey) };
    let mut sec1 = Vec::with_capacity(65);
    sec1.push(0x04);
    sec1.extend_from_slice(x);
    sec1.extend_from_slice(y);
    // Rejects points that aren't on the curve.
    VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| WebAuthnError::UnsupportedKey)
}

/// A software authenticator that makes responses the way a security key does, for tests.
#[cfg(test)]
pub(crate) mod testing {
    use p256::ecdsa::{signature::Signer, SigningKey};

    use super::*;

    pub(crate) struct SoftKey {
        pub key: SigningKey,
        pub credential_id: Vec<u8>,
        pub counter: u32,
        pub rp_id: String,
        pub origin: String,
    }

    impl SoftKey {
        pub(crate) fn new(seed: u8, rp_id: &str, origin: &str) -> Self {
            Self {
                key: SigningKey::from_bytes(&[seed; 32].into()).unwrap(),
                credential_id: vec![seed; 16],
                counter: 0,
                rp_id: rp_id.into(),
                origin: origin.into(),
            }
        }

        pub(crate) fn client_data(&self, kind: &str, challenge: &[u8]) -> Vec<u8> {
            serde_json::json!({ "type": kind, "challenge": URL_SAFE_NO_PAD.encode(challenge), "origin": self.origin })
                .to_string()
                .into_bytes()
        }

        fn auth_data_head(&self, flags: u8) -> Vec<u8> {
            let mut d = Sha256::digest(self.rp_id.as_bytes()).to_vec();
            d.push(flags);
            d.extend_from_slice(&self.counter.to_be_bytes());
            d
        }

        pub(crate) fn cose_key(&self) -> Value {
            let point = self.key.verifying_key().to_encoded_point(false);
            Value::Map(vec![
                (Value::from(1), Value::from(2)),
                (Value::from(3), Value::from(ES256)),
                (Value::from(-1), Value::from(1)),
                (Value::from(-2), Value::Bytes(point.x().unwrap().to_vec())),
                (Value::from(-3), Value::Bytes(point.y().unwrap().to_vec())),
            ])
        }

        /// `(clientDataJSON, attestationObject)` for a registration with `cose` as the key.
        pub(crate) fn register_with(&self, challenge: &[u8], cose: &Value) -> (Vec<u8>, Vec<u8>) {
            let mut auth = self.auth_data_head(FLAG_USER_PRESENT | FLAG_ATTESTED_CREDENTIAL);
            auth.extend_from_slice(&[0u8; 16]);
            auth.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
            auth.extend_from_slice(&self.credential_id);
            ciborium::into_writer(cose, &mut auth).unwrap();
            let object = Value::Map(vec![
                (Value::from("fmt"), Value::from("none")),
                (Value::from("attStmt"), Value::Map(vec![])),
                (Value::from("authData"), Value::Bytes(auth)),
            ]);
            let mut bytes = Vec::new();
            ciborium::into_writer(&object, &mut bytes).unwrap();
            (self.client_data("webauthn.create", challenge), bytes)
        }

        pub(crate) fn register(&self, challenge: &[u8]) -> (Vec<u8>, Vec<u8>) {
            self.register_with(challenge, &self.cose_key())
        }

        /// `(clientDataJSON, authenticatorData, signature)`; counts up first, as a key does.
        pub(crate) fn assert(&mut self, challenge: &[u8], flags: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
            self.counter += 1;
            let client = self.client_data("webauthn.get", challenge);
            let auth = self.auth_data_head(flags);
            let mut signed = auth.clone();
            signed.extend_from_slice(&Sha256::digest(&client));
            let sig: Signature = self.key.sign(&signed);
            (client, auth, sig.to_der().as_bytes().to_vec())
        }
    }

    pub(crate) const PRESENT: u8 = FLAG_USER_PRESENT;
}

#[cfg(test)]
mod tests {
    use super::{testing::*, *};

    const RP: &str = "portal.example.com";
    const ORIGIN: &str = "https://portal.example.com";

    fn rp() -> RelyingParty {
        RelyingParty::new(RP, [ORIGIN])
    }

    fn registered(key: &SoftKey) -> NewCredential {
        let (client, object) = key.register(b"register-challenge");
        rp().verify_registration(b"register-challenge", &client, &object).unwrap()
    }

    #[test]
    fn a_registered_key_signs_in_and_counts_forward() {
        let mut key = SoftKey::new(7, RP, ORIGIN);
        let cred = registered(&key);
        assert_eq!(cred.credential_id, key.credential_id);
        assert_eq!(cred.public_key, key.key.verifying_key().to_encoded_point(false).as_bytes());

        let (client, auth, sig) = key.assert(b"verify-1", PRESENT);
        let ok = rp().verify_assertion(b"verify-1", &cred.public_key, 0, &client, &auth, &sig).unwrap();
        assert_eq!(ok, Asserted { sign_count: 1, user_verified: false });

        // Replaying the same response against a stored counter of 1 fails.
        let err = rp().verify_assertion(b"verify-1", &cred.public_key, 1, &client, &auth, &sig).unwrap_err();
        assert_eq!(err, WebAuthnError::CounterRegressed);
    }

    #[test]
    fn everything_the_signature_covers_is_checked() {
        let mut key = SoftKey::new(7, RP, ORIGIN);
        let cred = registered(&key);
        let (client, auth, sig) = key.assert(b"verify", PRESENT);
        let check = |challenge: &[u8], client: &[u8], auth: &[u8], sig: &[u8]| {
            rp().verify_assertion(challenge, &cred.public_key, 0, client, auth, sig).unwrap_err()
        };

        assert_eq!(check(b"another", &client, &auth, &sig), WebAuthnError::WrongChallenge);
        let mut tampered = auth.clone();
        tampered[36] ^= 1; // the counter
        assert_eq!(check(b"verify", &client, &tampered, &sig), WebAuthnError::BadSignature);

        let mut phished = SoftKey::new(7, RP, "https://portal.example.com.evil.test");
        let (client, auth, sig) = phished.assert(b"verify", PRESENT);
        assert!(matches!(check(b"verify", &client, &auth, &sig), WebAuthnError::WrongOrigin(_)));

        let mut other_site = SoftKey::new(7, "evil.test", ORIGIN);
        let (client, auth, sig) = other_site.assert(b"verify", PRESENT);
        assert_eq!(check(b"verify", &client, &auth, &sig), WebAuthnError::WrongRelyingParty);

        let (client, auth, sig) = key.assert(b"verify", 0);
        assert_eq!(check(b"verify", &client, &auth, &sig), WebAuthnError::NoUserPresence);

        let mut someone_else = SoftKey::new(9, RP, ORIGIN);
        let (client, auth, sig) = someone_else.assert(b"verify", PRESENT);
        assert_eq!(check(b"verify", &client, &auth, &sig), WebAuthnError::BadSignature);

        // A registration response can't stand in for an assertion.
        let (client, _) = key.register(b"verify");
        let (_, auth, sig) = key.assert(b"verify", PRESENT);
        assert!(matches!(check(b"verify", &client, &auth, &sig), WebAuthnError::WrongType { .. }));
    }

    #[test]
    fn registration_accepts_only_es256_from_this_site() {
        let key = SoftKey::new(7, RP, ORIGIN);
        let (client, object) = key.register(b"c");
        assert_eq!(rp().verify_registration(b"other", &client, &object).unwrap_err(), WebAuthnError::WrongChallenge);

        let mut rs256 = key.cose_key();
        if let Value::Map(entries) = &mut rs256 {
            entries[1].1 = Value::from(-257);
        }
        let (client, object) = key.register_with(b"c", &rs256);
        assert_eq!(rp().verify_registration(b"c", &client, &object).unwrap_err(), WebAuthnError::UnsupportedKey);

        let mut off_curve = key.cose_key();
        if let Value::Map(entries) = &mut off_curve {
            entries[4].1 = Value::Bytes(vec![1; 32]);
        }
        let (client, object) = key.register_with(b"c", &off_curve);
        assert_eq!(rp().verify_registration(b"c", &client, &object).unwrap_err(), WebAuthnError::UnsupportedKey);

        let elsewhere = SoftKey::new(7, "evil.test", ORIGIN);
        let (client, object) = elsewhere.register(b"c");
        assert_eq!(rp().verify_registration(b"c", &client, &object).unwrap_err(), WebAuthnError::WrongRelyingParty);

        assert!(matches!(rp().verify_registration(b"c", &client, b"\xff\x00"), Err(WebAuthnError::Malformed(_))));
    }
}
