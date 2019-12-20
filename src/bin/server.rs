use spectrum_impl::{config, server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::InMemoryConfigStore::new();
    server::run(config_store).await
}
