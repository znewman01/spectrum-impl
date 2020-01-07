use config::store::InMemoryStore;
use spectrum_impl::{client, config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = InMemoryStore::new();
    client::run(config_store).await
}
