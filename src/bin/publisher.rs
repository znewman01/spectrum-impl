use spectrum_impl::{config, publisher, services::PublisherInfo};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let info = PublisherInfo::new();
    publisher::run(config_store, info).await
}
