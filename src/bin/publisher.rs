use spectrum_impl::{config, publisher, services::discovery::Service};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let service = Service::Publisher;
    publisher::run(config_store, service).await
}
