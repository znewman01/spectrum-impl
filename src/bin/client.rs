use spectrum_impl::{client, config, services::ClientInfo};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let info = ClientInfo::new(1);
    client::run(config_store, info).await
}
