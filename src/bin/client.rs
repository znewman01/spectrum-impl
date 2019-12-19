use spectrum_impl::{config, client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Box::new(config::InMemoryConfigStore::new());
    client::run(config_store).await
}
