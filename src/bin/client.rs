use spectrum_impl::{client, config};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Arc::new(config::InMemoryConfigStore::new());
    client::run(config_store).await
}
