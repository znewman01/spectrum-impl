use spectrum_impl::{client, config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::from_env()?;
    client::run(config_store).await
}
