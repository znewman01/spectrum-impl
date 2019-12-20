//! Spectrum implementation.
use std::time::Duration;

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
        server::run(config_store.clone(), tokio::time::delay_for(Duration::from_secs(5))),
        publisher::run(),
        leader::run()
    );

    Ok(())
}
