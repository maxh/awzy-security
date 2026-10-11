//! awzy-dev-idp: a development-only OpenID Connect provider for the local stack. Never deploy it.
//!
//! - Browser sign-in: authorization code + PKCE (S256). `/authorize` shows a "sign in as" page
//!   listing the dev users; no passwords.
//! - Agents: `client_credentials` for a service account acting on its own authority, and RFC 8693
//!   token exchange for a service account acting for a person (`sub` = person, `act.sub` =
//!   account).
//! - HTTPS, like a real IdP (see [`tls`]). The demo install (ADR 0032) runs it as plain HTTP behind
//!   its Gateway, which terminates TLS, under a path prefix: links on its pages are relative.
//! - Tokens are ES256 JWTs with `groups` and `name`, signed by a fixed key so they survive
//!   restarts. Their audience is `awzy`, or the `audience` asked for at `/token`: `awzy-mcp` for
//!   the portal's MCP endpoint, `awzy-gateway` for awzy-gateway (ADR 0081). Keys at `/jwks`, discovery at `/.well-known/openid-configuration`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use p256::{
    ecdsa::SigningKey,
    pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding},
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tower_http::cors::CorsLayer;

pub mod tls;

pub const AUDIENCE: &str = "awzy";
/// The audiences `/token` issues for: the portal, its MCP endpoint, and awzy-gateway.
pub const AUDIENCES: &[&str] = &[AUDIENCE, "awzy-mcp", "awzy-gateway"];
const KID: &str = "awzy-dev-1";
const TOKEN_LIFETIME: Duration = Duration::from_secs(3600);

/// The people you can sign in as: (id, name, groups).
pub const USERS: &[(&str, &str, &[&str])] = &[
    ("alice", "Alice (security)", &["security"]),
    ("bob", "Bob (engineering)", &["engineering"]),
    ("carol", "Carol (finance)", &["finance"]),
];

struct PendingCode {
    user: String,
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    nonce: Option<String>,
}

// A fixed key: tokens keep working across restarts. Development only.
fn signing_key() -> anyhow::Result<SigningKey> {
    SigningKey::from_bytes(&Sha256::digest(b"awzy-dev-idp signing key")).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Signs `claims` as the dev IdP signs its tokens, for tools that act as the demo's sample
/// people without signing in (awzy-synthetic's activity).
pub fn sign(claims: &Value) -> anyhow::Result<String> {
    let key = EncodingKey::from_ec_pem(signing_key()?.to_pkcs8_pem(LineEnding::LF)?.as_bytes())?;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(KID.into());
    Ok(encode(&header, claims, &key)?)
}

#[derive(Clone)]
pub struct DevIdp(Arc<Inner>);

struct Inner {
    issuer: String,
    encoding: EncodingKey,
    decoding: DecodingKey,
    jwk: Value,
    codes: Mutex<HashMap<String, PendingCode>>,
}

impl DevIdp {
    pub fn new(issuer: String) -> anyhow::Result<Self> {
        let key = signing_key()?;
        let encoding = EncodingKey::from_ec_pem(key.to_pkcs8_pem(LineEnding::LF)?.as_bytes())?;
        let decoding = DecodingKey::from_ec_pem(key.verifying_key().to_public_key_pem(LineEnding::LF)?.as_bytes())?;
        let point = key.verifying_key().to_encoded_point(false);
        let jwk = json!({
            "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": KID,
            "x": URL_SAFE_NO_PAD.encode(point.x().expect("uncompressed point")),
            "y": URL_SAFE_NO_PAD.encode(point.y().expect("uncompressed point")),
        });
        Ok(Self(Arc::new(Inner { issuer, encoding, decoding, jwk, codes: Mutex::new(HashMap::new()) })))
    }

    /// A signed access token. `act`: the service account acting for `sub`.
    pub fn token(&self, sub: &str, act: Option<&str>, extra: Value) -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let mut claims = json!({
            "iss": self.0.issuer, "aud": AUDIENCE, "sub": sub, "iat": now, "exp": now + TOKEN_LIFETIME.as_secs(),
        });
        if let Some((_, name, groups)) = USERS.iter().find(|(id, ..)| *id == sub) {
            claims["name"] = json!(name);
            claims["groups"] = json!(groups);
        }
        if let Some(a) = act {
            claims["act"] = json!({ "sub": a });
        }
        if let Some(extra) = extra.as_object() {
            claims.as_object_mut().expect("object").extend(extra.clone());
        }
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(KID.into());
        encode(&header, &claims, &self.0.encoding).expect("signing with a valid key")
    }

    fn validate(&self, token: &str) -> Option<Value> {
        let mut v = Validation::new(Algorithm::ES256);
        v.set_issuer(&[&self.0.issuer]);
        v.set_audience(AUDIENCES);
        decode::<Value>(token, &self.0.decoding, &v).ok().map(|d| d.claims)
    }
}

pub fn router(idp: DevIdp) -> Router {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/jwks", get(jwks))
        .route("/authorize", get(authorize))
        .route("/authorize/as", get(authorize_as))
        .route("/token", post(token))
        .layer(CorsLayer::permissive())
        .with_state(idp)
}

async fn discovery(State(idp): State<DevIdp>) -> Json<Value> {
    let i = &idp.0.issuer;
    Json(json!({
        "issuer": i,
        "authorization_endpoint": format!("{i}/authorize"),
        "token_endpoint": format!("{i}/token"),
        "jwks_uri": format!("{i}/jwks"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "client_credentials", "urn:ietf:params:oauth:grant-type:token-exchange"],
        "code_challenge_methods_supported": ["S256"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["ES256"],
        "scopes_supported": ["openid", "profile"],
    }))
}

async fn jwks(State(idp): State<DevIdp>) -> Json<Value> {
    Json(json!({ "keys": [idp.0.jwk] }))
}

#[derive(Deserialize)]
struct AuthorizeParams {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    #[serde(default)]
    state: Option<String>,
    code_challenge: String,
    code_challenge_method: String,
    #[serde(default)]
    nonce: Option<String>,
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

async fn authorize(Query(p): Query<AuthorizeParams>, raw: axum::extract::RawQuery) -> Response {
    if p.response_type != "code" || p.code_challenge_method != "S256" {
        return (StatusCode::BAD_REQUEST, "only response_type=code with PKCE S256 is supported").into_response();
    }
    let query = raw.0.unwrap_or_default();
    let buttons: String = USERS
        .iter()
        .map(|(id, name, groups)| {
            format!(
                r#"<a class="user" href="authorize/as?user={id}&amp;{q}"><b>{name}</b><span>{g}</span></a>"#,
                q = escape(&query),
                name = escape(name),
                g = escape(&groups.join(", "))
            )
        })
        .collect();
    Html(format!(
        r#"<!doctype html><meta charset="utf-8"><title>awzy dev sign-in</title>
<style>body{{font:15px system-ui;max-width:420px;margin:60px auto;padding:0 16px;color:#17212b}}
.user{{display:flex;justify-content:space-between;padding:12px 14px;margin:8px 0;border:1px solid #d9e0e5;border-radius:6px;color:inherit;text-decoration:none}}
.user:hover{{border-color:#0d6b66}}span{{color:#5a6875}}p{{color:#9a5b00}}</style>
<h2>Sign in to awzy (development)</h2><p>Development identity provider. No passwords.</p>{buttons}"#
    ))
    .into_response()
}

#[derive(Deserialize)]
struct AsParams {
    user: String,
    #[serde(flatten)]
    auth: AuthorizeParams,
}

async fn authorize_as(State(idp): State<DevIdp>, Query(p): Query<AsParams>) -> Response {
    if !USERS.iter().any(|(id, ..)| *id == p.user) {
        return (StatusCode::BAD_REQUEST, "unknown dev user").into_response();
    }
    let code = uuid::Uuid::now_v7().simple().to_string();
    idp.0.codes.lock().expect("codes lock").insert(
        code.clone(),
        PendingCode {
            user: p.user,
            client_id: p.auth.client_id,
            redirect_uri: p.auth.redirect_uri.clone(),
            code_challenge: p.auth.code_challenge,
            nonce: p.auth.nonce,
        },
    );
    let sep = if p.auth.redirect_uri.contains('?') { '&' } else { '?' };
    let state = p.auth.state.map(|s| format!("&state={}", urlencode(&s))).unwrap_or_default();
    Redirect::to(&format!("{}{sep}code={code}{state}", p.auth.redirect_uri)).into_response()
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[derive(Deserialize)]
struct TokenParams {
    grant_type: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    code_verifier: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    /// Token exchange: the person's access token.
    #[serde(default)]
    subject_token: Option<String>,
    /// Token exchange: the service account that will act for them. (A real IdP takes an
    /// `actor_token`; the dev IdP takes the account ID.)
    #[serde(default)]
    actor: Option<String>,
    /// One of [`AUDIENCES`]; default `awzy`.
    #[serde(default)]
    audience: Option<String>,
}

fn oauth_error(error: &str, description: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": error, "error_description": description }))).into_response()
}

fn token_response(access: String, id_token: Option<String>) -> Response {
    let mut body = json!({ "access_token": access, "token_type": "Bearer", "expires_in": TOKEN_LIFETIME.as_secs() });
    if let Some(id) = id_token {
        body["id_token"] = json!(id);
    }
    ([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

async fn token(State(idp): State<DevIdp>, Form(p): Form<TokenParams>) -> Response {
    let audience = p.audience.clone().unwrap_or_else(|| AUDIENCE.to_string());
    if !AUDIENCES.contains(&audience.as_str()) {
        return oauth_error("invalid_target", &format!("audience is one of {AUDIENCES:?}"));
    }
    let aud = json!({ "aud": audience });
    match p.grant_type.as_str() {
        "authorization_code" => {
            let (Some(code), Some(verifier)) = (p.code, p.code_verifier) else {
                return oauth_error("invalid_request", "code and code_verifier are required");
            };
            let Some(pending) = idp.0.codes.lock().expect("codes lock").remove(&code) else {
                return oauth_error("invalid_grant", "unknown or used code");
            };
            if p.redirect_uri.as_deref() != Some(pending.redirect_uri.as_str())
                || p.client_id.as_deref() != Some(pending.client_id.as_str())
            {
                return oauth_error(
                    "invalid_grant",
                    "redirect_uri or client_id doesn't match the authorization request",
                );
            }
            if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) != pending.code_challenge {
                return oauth_error("invalid_grant", "PKCE code_verifier doesn't match");
            }
            let id_token = idp.token(&pending.user, None, json!({ "nonce": pending.nonce, "azp": pending.client_id }));
            token_response(idp.token(&pending.user, None, aud), Some(id_token))
        }
        "client_credentials" => match p.client_id {
            // RFC 9068: client_id equal to sub marks the application's own token.
            Some(account) => {
                token_response(idp.token(&account, None, json!({ "client_id": account, "aud": audience })), None)
            }
            None => oauth_error("invalid_request", "client_id (the service account) is required"),
        },
        "urn:ietf:params:oauth:grant-type:token-exchange" => {
            let (Some(subject_token), Some(actor)) = (p.subject_token, p.actor) else {
                return oauth_error("invalid_request", "subject_token and actor are required");
            };
            match idp.validate(&subject_token).and_then(|c| c["sub"].as_str().map(str::to_string)) {
                Some(human) => token_response(idp.token(&human, Some(&actor), aud), None),
                None => oauth_error("invalid_grant", "subject_token is not a valid token from this IdP"),
            }
        }
        other => oauth_error("unsupported_grant_type", other),
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    use super::*;

    fn idp() -> DevIdp {
        DevIdp::new("http://idp.test".into()).unwrap()
    }

    #[test]
    fn tokens_signed_outside_the_idp_are_its_own() {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let token = sign(&json!({ "iss": "http://idp.test", "aud": AUDIENCE, "sub": "bob", "exp": now + 60 })).unwrap();
        assert_eq!(idp().validate(&token).unwrap()["sub"], "bob");
    }

    async fn post_form(app: Router, form: &str) -> (StatusCode, Value) {
        let res = app
            .oneshot(
                Request::post("/token")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(form.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn authorization_code_with_pkce() {
        let idp = idp();
        let app = router(idp.clone());
        let verifier = "a-long-random-pkce-verifier-0123456789";
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let q = format!(
            "user=alice&client_id=portal&redirect_uri=http%3A%2F%2Fapp%2Fcb&response_type=code&state=s1&code_challenge={challenge}&code_challenge_method=S256"
        );
        let res =
            app.clone().oneshot(Request::get(format!("/authorize/as?{q}")).body(Body::empty()).unwrap()).await.unwrap();
        let location = res.headers()["location"].to_str().unwrap().to_string();
        assert!(location.starts_with("http://app/cb?code=") && location.ends_with("&state=s1"), "{location}");
        let code = location.split("code=").nth(1).unwrap().split('&').next().unwrap();

        let (status, bad) = post_form(app.clone(), &format!("grant_type=authorization_code&code={code}&code_verifier=wrong&redirect_uri=http%3A%2F%2Fapp%2Fcb&client_id=portal")).await;
        assert_eq!((status, bad["error"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_grant")));

        // The failed attempt used the code up; get a fresh one.
        let res =
            app.clone().oneshot(Request::get(format!("/authorize/as?{q}")).body(Body::empty()).unwrap()).await.unwrap();
        let location = res.headers()["location"].to_str().unwrap().to_string();
        let code = location.split("code=").nth(1).unwrap().split('&').next().unwrap();
        let (status, ok) = post_form(app, &format!("grant_type=authorization_code&code={code}&code_verifier={verifier}&redirect_uri=http%3A%2F%2Fapp%2Fcb&client_id=portal")).await;
        assert_eq!(status, StatusCode::OK);
        let claims = idp.validate(ok["access_token"].as_str().unwrap()).unwrap();
        assert_eq!((claims["sub"].as_str(), claims["groups"][0].as_str()), (Some("alice"), Some("security")));
    }

    #[tokio::test]
    async fn token_exchange_names_the_account_and_the_person() {
        let idp = idp();
        let human = idp.token("bob", None, json!({}));
        let (status, res) = post_form(
            router(idp.clone()),
            &format!("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Atoken-exchange&subject_token={human}&actor=sales-genie"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let claims = idp.validate(res["access_token"].as_str().unwrap()).unwrap();
        assert_eq!((claims["sub"].as_str(), claims["act"]["sub"].as_str()), (Some("bob"), Some("sales-genie")));
    }

    #[tokio::test]
    async fn tokens_are_for_the_audience_asked_for() {
        let idp = idp();
        let app = router(idp.clone());
        let (_, gateway) =
            post_form(app.clone(), "grant_type=client_credentials&client_id=bot&audience=awzy-gateway").await;
        assert_eq!(idp.validate(gateway["access_token"].as_str().unwrap()).unwrap()["aud"], "awzy-gateway");
        let (_, portal) = post_form(app.clone(), "grant_type=client_credentials&client_id=bot").await;
        assert_eq!(idp.validate(portal["access_token"].as_str().unwrap()).unwrap()["aud"], "awzy");
        let (status, other) = post_form(app, "grant_type=client_credentials&client_id=bot&audience=elsewhere").await;
        assert_eq!((status, other["error"].as_str()), (StatusCode::BAD_REQUEST, Some("invalid_target")));
    }
}
