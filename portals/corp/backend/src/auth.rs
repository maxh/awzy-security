//! Corp portal sign-in with usernames and passwords.
//!
//! FOR DEMOS AND TESTING ONLY (ADR 0025). Staff accounts with passwords are a stopgap so the
//! corp portal can be shown and tested before awzy has its own IdP. Before the corp portal
//! controls releases, the fleet, or support sessions, this is replaced by SSO against awzy's
//! IdP with security keys. Don't build on it.
//!
//! What it does, so even a demo deployment isn't trivially open:
//! - Accounts come from a file (`AWZY_CORP_PORTAL_ACCOUNTS_FILE`) of argon2id hashes made with
//!   `awzy-corp-user`. No sign-up, no password reset.
//! - A session is a cookie signed with HMAC-SHA256 (`AWZY_CORP_PORTAL_SESSION_SECRET`), HttpOnly,
//!   SameSite=Strict, Secure, valid for 12 hours. There is no server-side revocation:
//!   changing the secret signs everyone out.
//! - After 5 failed attempts a username is locked for 15 minutes. Unknown usernames take as
//!   long as known ones and get the same answer.
//! - `/graphql` POSTs need `content-type: application/json`, so a cross-site form can't send one
//!   even if a browser ignored SameSite.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const COOKIE: &str = "awzy_corp_session";
const SESSION_LIFETIME: Duration = Duration::from_secs(12 * 3600);
const MAX_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

/// One line of the accounts file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffAccount {
    pub username: String,
    pub name: String,
    /// argon2id PHC string, from `awzy-corp-user`.
    pub password_hash: String,
}

/// The signed-in staff member of one request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Staff {
    pub username: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
struct SessionClaims {
    sub: String,
    name: String,
    exp: u64,
}

/// argon2id with the crate's defaults (19 MiB, 2 passes).
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default().hash_password(password.as_bytes(), &salt).map_err(|e| anyhow::anyhow!("{e}"))?.to_string())
}

#[derive(Clone)]
pub struct Auth(Arc<Inner>);

struct Inner {
    accounts: HashMap<String, StaffAccount>,
    secret: Vec<u8>,
    secure_cookies: bool,
    /// Verified against for unknown usernames, so they take as long as known ones.
    dummy_hash: String,
    failures: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Auth {
    pub fn new(accounts: Vec<StaffAccount>, secret: Vec<u8>, secure_cookies: bool) -> anyhow::Result<Self> {
        anyhow::ensure!(secret.len() >= 32, "the session secret must be at least 32 bytes");
        for a in &accounts {
            PasswordHash::new(&a.password_hash)
                .map_err(|e| anyhow::anyhow!("account {:?}: bad password_hash: {e}", a.username))?;
        }
        Ok(Self(Arc::new(Inner {
            accounts: accounts.into_iter().map(|a| (a.username.clone(), a)).collect(),
            secret,
            secure_cookies,
            dummy_hash: hash_password("not a real password")?,
            failures: Mutex::new(HashMap::new()),
        })))
    }

    /// AWZY_CORP_PORTAL_ACCOUNTS_FILE (JSON list of accounts), AWZY_CORP_PORTAL_SESSION_SECRET (at least 32
    /// bytes), and AWZY_CORP_PORTAL_INSECURE_COOKIES=1 to drop `Secure` for plain-HTTP local runs.
    pub fn from_env() -> anyhow::Result<Self> {
        let var = |k: &str| std::env::var(k).map_err(|_| anyhow::anyhow!("{k} is not set"));
        let path = var("AWZY_CORP_PORTAL_ACCOUNTS_FILE")?;
        let accounts: Vec<StaffAccount> = serde_json::from_slice(&std::fs::read(&path)?)?;
        let secure = std::env::var("AWZY_CORP_PORTAL_INSECURE_COOKIES").as_deref() != Ok("1");
        tracing::warn!(
            accounts = accounts.len(),
            "corp portal sign-in with passwords: for demos and testing only (ADR 0025)"
        );
        Self::new(accounts, var("AWZY_CORP_PORTAL_SESSION_SECRET")?.into_bytes(), secure)
    }

    fn mac(&self) -> Hmac<Sha256> {
        Hmac::<Sha256>::new_from_slice(&self.0.secret).expect("HMAC takes any key length")
    }

    fn issue(&self, staff: &Staff) -> String {
        let exp = now() + SESSION_LIFETIME.as_secs();
        let claims = SessionClaims { sub: staff.username.clone(), name: staff.name.clone(), exp };
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).expect("serializable"));
        let mut mac = self.mac();
        mac.update(body.as_bytes());
        format!("{body}.{}", URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }

    /// The staff member a session cookie names, if it is ours, unexpired, and the account
    /// still exists.
    fn verify(&self, value: &str) -> Option<Staff> {
        let (body, sig) = value.split_once('.')?;
        let mut mac = self.mac();
        mac.update(body.as_bytes());
        mac.verify_slice(&URL_SAFE_NO_PAD.decode(sig).ok()?).ok()?;
        let claims: SessionClaims = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).ok()?).ok()?;
        let account = self.0.accounts.get(&claims.sub)?;
        (claims.exp > now()).then(|| Staff { username: account.username.clone(), name: account.name.clone() })
    }

    fn session_from(&self, headers: &HeaderMap) -> Option<Staff> {
        let cookies = headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok());
        cookies
            .flat_map(|c| c.split(';'))
            .filter_map(|kv| kv.trim().split_once('='))
            .find(|(k, _)| *k == COOKIE)
            .and_then(|(_, v)| self.verify(v))
    }

    fn cookie(&self, value: &str, max_age: u64) -> HeaderValue {
        let secure = if self.0.secure_cookies { "; Secure" } else { "" };
        HeaderValue::from_str(&format!(
            "{COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{secure}"
        ))
        .expect("cookie is ASCII")
    }

    fn locked(&self, username: &str) -> bool {
        let failures = self.0.failures.lock().expect("failures lock");
        matches!(failures.get(username), Some((n, since)) if *n >= MAX_FAILURES && since.elapsed() < LOCKOUT)
    }

    fn record(&self, username: &str, ok: bool) {
        let mut failures = self.0.failures.lock().expect("failures lock");
        if ok {
            failures.remove(username);
            return;
        }
        let entry = failures.entry(username.to_string()).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= LOCKOUT {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
    }
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[derive(Deserialize)]
struct Login {
    username: String,
    password: String,
}

async fn login(State(auth): State<Auth>, Json(req): Json<Login>) -> Response {
    if auth.locked(&req.username) {
        return (StatusCode::TOO_MANY_REQUESTS, "Too many failed attempts. Try again in 15 minutes.").into_response();
    }
    let account = auth.0.accounts.get(&req.username).cloned();
    let hash = account.as_ref().map(|a| a.password_hash.clone()).unwrap_or_else(|| auth.0.dummy_hash.clone());
    let password = req.password;
    // argon2 is deliberately slow: keep it off the async workers.
    let matches = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    let Some(account) = account.filter(|_| matches) else {
        auth.record(&req.username, false);
        tracing::warn!(username = %req.username, "corp portal sign-in failed");
        return (StatusCode::UNAUTHORIZED, "Wrong username or password.").into_response();
    };
    auth.record(&req.username, true);
    tracing::info!(username = %account.username, "corp portal sign-in");
    let staff = Staff { username: account.username, name: account.name };
    let cookie = auth.cookie(&auth.issue(&staff), SESSION_LIFETIME.as_secs());
    ([(header::SET_COOKIE, cookie)], Json(staff)).into_response()
}

async fn logout(State(auth): State<Auth>) -> Response {
    ([(header::SET_COOKIE, auth.cookie("", 0))], StatusCode::NO_CONTENT).into_response()
}

/// `/auth/login` and `/auth/logout`: starting and ending a session. Who is signed in comes from
/// the GraphQL `viewer` query, which answers 401 without a session.
pub fn routes(auth: Auth) -> Router {
    Router::new().route("/auth/login", post(login)).route("/auth/logout", post(logout)).with_state(auth)
}

/// Middleware: a valid session and a JSON body, or 401/415. Puts [`Staff`] in the request's
/// extensions.
pub async fn require_staff(State(auth): State<Auth>, mut req: Request, next: Next) -> Response {
    let json = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/json"));
    if !json {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "send application/json").into_response();
    }
    match auth.session_from(req.headers()) {
        Some(staff) => {
            req.extensions_mut().insert(staff);
            next.run(req).await
        }
        None => (StatusCode::UNAUTHORIZED, "sign in first").into_response(),
    }
}
