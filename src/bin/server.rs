use spectrum_impl::{config, server};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = Arc::new(config::InMemoryConfigStore::new());
    server::run(config_store).await
}
