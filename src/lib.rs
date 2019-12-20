//! Spectrum implementation.
use std::time::Duration;
use tokio::time::timeout;

pub mod client;
pub mod leader;
pub mod publisher;
pub mod server;

pub mod config;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::InMemoryConfigStore::new();
    let _ = futures::join!(
        client::run(config_store.clone()),
        client::run(config_store.clone()),
        timeout(Duration::from_secs(5), server::run(config_store.clone())),
        publisher::run(),
        leader::run()
    );

    Ok(())
}
