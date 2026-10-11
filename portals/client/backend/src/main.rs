#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-client-portal");
    // Host headers the MCP endpoint accepts, comma-separated, e.g. "portal.example.com".
    // Unset: loopback only.
    let mcp_hosts = std::env::var("AWZY_MCP_ALLOWED_HOSTS")
        .map(|v| v.split(',').map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect())
        .unwrap_or_default();
    let mut services = awzy_client_portal::Services::from_env()?;
    services.bot = awzy_client_portal::awzy_bot::model_from_env().await;
    let auth = awzy_client_portal::access::Auth::from_env()?;
    let mcp_auth = awzy_client_portal::access::Auth::from_env_for_mcp()?;
    let mut app = awzy_client_portal::router(services, auth, mcp_auth, mcp_hosts);
    if let Some(dir) = awzy_portal::frontend_dir_from_env() {
        app = awzy_portal::with_frontend(app, &dir);
    }
    awzy_server::serve("awzy-client-portal", app).await
}
