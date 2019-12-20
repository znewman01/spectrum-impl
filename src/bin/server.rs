use futures::prelude::*;
use spectrum_impl::{config, server};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::InMemoryConfigStore::new();
    server::run(config_store, tokio::signal::ctrl_c().map(|_| ())).await
}
