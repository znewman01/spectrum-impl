//! Spectrum implementation.
use futures::future::FutureExt;

pub mod client;
pub mod leader;
pub mod publisher;
pub mod worker;

pub mod config;
mod quorum;

use log::trace;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::from_env()?;
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
