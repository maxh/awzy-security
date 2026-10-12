//! Apps an admin trusts to sign tokens for their own agents, besides the client's IdP (ADR 0110).

use serde::{Deserialize, Serialize};

/// An app that signs tokens for the service accounts under its prefix, its name and a dot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedIssuer {
    /// The app's name: the `sub` of the tokens it signs for itself, and its agents' prefix.
    pub name: String,
    /// The `iss` its tokens carry.
    pub issuer: String,
    /// Where its signing keys are published.
    pub jwks_url: String,
    /// The `aud` its tokens must carry.
    pub audience: String,
}

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("{0}")]
pub struct InvalidIssuer(String);

impl TrustedIssuer {
    /// What its agents' IDs start with: `acme-crm.` for `acme-crm`. Names have no dots, so no
    /// two apps' prefixes overlap.
    pub fn agent_prefix(&self) -> String {
        format!("{}.", self.name)
    }

    /// Whether it may sign tokens for this service account: one under its prefix.
    pub fn may_sign_for(&self, service_account: &str) -> bool {
        service_account.strip_prefix(&self.agent_prefix()).is_some_and(|rest| !rest.is_empty())
    }

    pub fn validate(&self) -> Result<(), InvalidIssuer> {
        let bad = |m: &str| Err(InvalidIssuer(m.into()));
        if self.name.is_empty()
            || self.name.len() > 64
            || !self.name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return bad("the name is 1 to 64 lowercase letters, digits, and dashes");
        }
        if self.issuer.trim().is_empty() || self.issuer.len() > 500 {
            return bad("the issuer is the `iss` its tokens carry, at most 500 characters");
        }
        if !self.jwks_url.starts_with("https://") || self.jwks_url.len() > 500 {
            return bad("the JWKS URL is https://, at most 500 characters");
        }
        if self.audience.trim().is_empty() || self.audience.len() > 200 {
            return bad("the audience is the `aud` its tokens must carry, at most 200 characters");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> TrustedIssuer {
        TrustedIssuer {
            name: "acme-crm".into(),
            issuer: "https://crm.acme.example".into(),
            jwks_url: "https://crm.acme.example/.well-known/jwks.json".into(),
            audience: "awzy".into(),
        }
    }

    #[test]
    fn an_app_signs_only_for_agents_under_its_prefix() {
        let app = app();
        assert!(app.may_sign_for("acme-crm.renewals-bot"));
        assert!(!app.may_sign_for("acme-crm."));
        assert!(!app.may_sign_for("acme-crm"));
        assert!(!app.may_sign_for("acme-crm-two.bot"));
        assert!(!app.may_sign_for("sales-genie"));
    }

    #[test]
    fn a_name_with_a_dot_or_a_plain_http_jwks_url_is_refused() {
        assert_eq!(app().validate(), Ok(()));
        assert!(TrustedIssuer { name: "acme.crm".into(), ..app() }.validate().is_err());
        assert!(TrustedIssuer { jwks_url: "http://crm.acme.example/jwks".into(), ..app() }.validate().is_err());
        assert!(TrustedIssuer { audience: " ".into(), ..app() }.validate().is_err());
    }
}
