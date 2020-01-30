//! Spectrum implementation.
use futures::future::FutureExt;
use std::sync::Arc;
use tokio::sync::Barrier;

pub mod client;
pub mod crypto;
pub mod leader;
pub mod publisher;
pub mod worker;

pub mod config;
mod experiment;
mod services;

use experiment::Experiment;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_store = config::from_env()?;
    let experiment = Experiment::new();
    experiment::write_to_store(&config_store, experiment).await?;
    let barrier = Arc::new(Barrier::new(2));
    let barrier2 = barrier.clone();
    let shutdown = async move {
        barrier2.wait().await;
    };
    let _ = futures::join!(
        client::run(config_store.clone()).then(|_| { barrier.wait() }),
        worker::run(config_store.clone(), shutdown),
        publisher::run(config_store.clone()),
        leader::run(&config_store)
    );

    Ok(())
}
