use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-dev-idp");
    let addr: SocketAddr = std::env::var("AWZY_LISTEN_ADDR").unwrap_or_else(|_| "127.0.0.1:5556".into()).parse()?;
    let issuer = std::env::var("AWZY_DEV_IDP_ISSUER").unwrap_or_else(|_| format!("https://{addr}"));
    tracing::warn!(%issuer, "development identity provider: anyone can sign in, and its signing key is public; only for local development and the demo install (ADR 0032)");
    let app = awzy_dev_idp::router(awzy_dev_idp::DevIdp::new(issuer)?);
    // AWZY_DEV_IDP_TLS=off: plain HTTP, for the demo install, where its Gateway terminates TLS.
    // `serve` adds the health routes itself.
    if std::env::var("AWZY_DEV_IDP_TLS").as_deref() == Ok("off") {
        return awzy_server::serve("awzy-dev-idp", app).await;
    }
    let tls = awzy_dev_idp::tls::load_or_generate()?;
    awzy_dev_idp::tls::serve(awzy_server::with_health(app), addr, &tls).await
}
