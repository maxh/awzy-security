#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-client-portal");
    // Host headers the MCP endpoint accepts, comma-separated, e.g. "portal.example.com".
    // Unset: loopback only.
    let mcp_hosts = std::env::var("AWZY_MCP_ALLOWED_HOSTS")
        .map(|v| v.split(',').map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect())
        .unwrap_or_default();
    // true in a client's install (the chart's default): GraphQL runs only the frontend's own
    // queries (ADR 0088). awzy's staging and demo installs may turn it off for local frontends
    // (ADR 0024).
    let persisted_only = match std::env::var("AWZY_PORTAL_PERSISTED_QUERIES_ONLY").as_deref() {
        Ok("true") => true,
        Ok("false") | Err(_) => false,
        Ok(other) => anyhow::bail!("AWZY_PORTAL_PERSISTED_QUERIES_ONLY is true or false, not {other:?}"),
    };
    let mut services = awzy_client_portal::Services::from_env()?;
    services.bot = awzy_client_portal::awzy_bot::model_from_env().await;
    // awzy staff in a support session a client admin approved sign in with the corp portal's
    // token (ADR 0095); without AWZY_SUPPORT_PUBLIC_KEY_FILE, they can't.
    let support = awzy_client_portal::support::SupportSignIn::from_env(services.challenge.clone())?;
    let auth = awzy_client_portal::access::Auth::from_env()?.with_support(support);
    let mcp_auth = awzy_client_portal::access::Auth::from_env_for_mcp()?;
    // Apps trusted to sign tokens for their own agents (ADR 0110), from the PDP.
    if let Some(pdp) = services.pdp.clone() {
        let (a, m) = (auth.clone(), mcp_auth.clone());
        tokio::spawn(async move { awzy_client_portal::agents::follow_trusted_issuers(pdp, &[&a, &m]).await });
    }
    // The built frontend carries the queries it sends by ID (ADR 0090).
    let frontend = awzy_portal::frontend_dir_from_env();
    let persisted = awzy_portal::PersistedQueries::from_frontend(frontend.as_deref(), persisted_only)?;
    let mut app = awzy_client_portal::router(services, auth, mcp_auth, mcp_hosts, persisted);
    if let Some(dir) = frontend {
        app = awzy_portal::with_frontend(app, &dir);
    }
    awzy_server::serve("awzy-client-portal", app).await
}
