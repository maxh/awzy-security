//! awzy-registry                                  serve (needs AWZY_POSTGRES_URL, AWZY_REGISTRY_UPSTREAM)
//! awzy-registry token create <install_id> <by>   print a new pull token for an install, once
//! awzy-registry token revoke <install_id>        remove an install's pull token

use anyhow::{bail, Context};
use awzy_model::InstallId;
use awzy_registry::{router, tokens::TokenStore, upstream::Upstream, Registry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    awzy_telemetry::init("awzy-registry");
    let var = |k: &str| std::env::var(k).with_context(|| format!("{k} is not set"));
    // Its own schema in the corp portal's database, so the two sets of migrations stay apart.
    let store = TokenStore::connect(&var("AWZY_POSTGRES_URL")?, "registry").await?;

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => {
            // e.g. 123456789012.dkr.ecr.us-east-2.amazonaws.com
            let upstream = Upstream::ecr(&var("AWZY_REGISTRY_UPSTREAM")?)?;
            let realm = std::env::var("AWZY_REGISTRY_REALM").unwrap_or_else(|_| "registry.awzysecurity.com".into());
            awzy_server::serve("awzy-registry", router(Registry { tokens: store, upstream, realm })).await
        }
        ["token", "create", install, by] => {
            let install: InstallId = install.parse()?;
            let token = store.create(install, by).await?;
            println!("{token}");
            eprintln!("Pull token for {install}. Shown once: store it in the install's values (username {install}).");
            Ok(())
        }
        ["token", "revoke", install] => {
            let install: InstallId = install.parse()?;
            if !store.revoke(install).await? {
                bail!("{install} had no pull token");
            }
            eprintln!("Revoked the pull token for {install}; it stops working within a minute.");
            Ok(())
        }
        _ => bail!("usage: awzy-registry [token create <install_id> <by> | token revoke <install_id>]"),
    }
}
