use config::store::InMemoryStore;
use futures::prelude::*;
use spectrum_impl::{config, worker};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = InMemoryStore::new();
    worker::run(config_store, tokio::signal::ctrl_c().map(|_| ())).await
}
