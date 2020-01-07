//! Spectrum implementation.
use futures::future::FutureExt;

pub mod client;
pub mod leader;
pub mod publisher;
pub mod worker;

pub mod config;

use config::store::InMemoryStore;
use log::trace;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = InMemoryStore::new();
    let barrier = tokio::sync::Barrier::new(2);
    let _ = futures::join!(
        client::run(config_store.clone()).then(|_| {
            trace!("Awaiting barrier -- client.");
            barrier.wait()
        }),
        worker::run(config_store.clone(), barrier.wait().map(|_| ())),
        publisher::run(),
        leader::run()
    );

    Ok(())
}
