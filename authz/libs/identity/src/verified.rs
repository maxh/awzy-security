//! Tokens whose signature, issuer, and audience have been checked, until they expire. An agent
//! sends the same token with every call of a session, and checking an RS256 or ES256 signature
//! costs more than the rest of the call's work in the gateway. As with the PDP's service tokens
//! (ADR 0055), a key the IdP drops stops being honored as its tokens expire.

use std::{
    collections::HashMap,
    sync::RwLock,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::Claims;

/// Tokens kept before the cache starts over: well above the callers one gateway sees at once.
const CAPACITY: usize = 10_000;

#[derive(Default)]
pub(crate) struct Verified(RwLock<HashMap<String, Claims>>);

impl Verified {
    /// The claims of `token`, if it was verified and hasn't expired.
    pub(crate) fn get(&self, token: &str) -> Option<Claims> {
        let verified = self.0.read().unwrap_or_else(|e| e.into_inner());
        verified.get(token).filter(|claims| !expired(claims)).cloned()
    }

    pub(crate) fn insert(&self, token: &str, claims: &Claims) {
        if expired(claims) {
            return;
        }
        let mut verified = self.0.write().unwrap_or_else(|e| e.into_inner());
        if verified.len() >= CAPACITY {
            verified.retain(|_, c| !expired(c));
            if verified.len() >= CAPACITY {
                verified.clear();
            }
        }
        verified.insert(token.to_string(), claims.clone());
    }
}

/// Past `exp`, with none of the leeway verifying allows: once cached, a token is used no longer
/// than it says.
fn expired(claims: &Claims) -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(f64::MAX, |d| d.as_secs_f64());
    now >= claims.exp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(exp: f64) -> Claims {
        serde_json::from_value(serde_json::json!({ "sub": "alice", "exp": exp })).unwrap()
    }

    fn now() -> f64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
    }

    #[test]
    fn a_verified_token_is_remembered_until_it_expires() {
        let verified = Verified::default();
        verified.insert("live", &claims(now() + 60.0));
        verified.insert("expired", &claims(now() - 1.0));
        assert_eq!(verified.get("live").map(|c| c.sub), Some("alice".to_string()));
        assert!(verified.get("expired").is_none(), "an expired token is never served");
        assert!(verified.get("unknown").is_none());
    }
}
