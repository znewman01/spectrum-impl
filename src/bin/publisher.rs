use spectrum_impl::{config, publisher};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::from_env()?;
    publisher::run(config_store).await
}
