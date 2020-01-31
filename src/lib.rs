//! Spectrum implementation.
use futures::{FutureExt, TryFutureExt, TryStreamExt};
use log::debug;
use std::sync::Arc;
use tokio::sync::Barrier;

pub mod client;
pub mod crypto;
pub mod leader;
pub mod publisher;
pub mod worker;

pub mod config;
mod experiment;
mod net;
mod services;

use experiment::Experiment;
use services::discovery::Service::{Leader, Publisher, Worker};

pub async fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    let experiment = Experiment::new(1, 1);
    experiment::write_to_store(&config_store, experiment).await?;
    let barrier = Arc::new(Barrier::new(1 + experiment.iter_services().count()));
    debug!("barrier: {:?}", barrier);
    let handles = futures::stream::FuturesUnordered::new();

    // TODO(zjn): support many clients
    for _ in 0..1 {
        let barrier = barrier.clone();
        let shutdown = async move {
            barrier.wait().await;
            Ok(())
        };
        handles.push(
            client::run(config_store.clone())
                .and_then(|_| shutdown)
                .boxed(),
        );
    }

    for service in experiment.iter_services() {
        let barrier = barrier.clone();
        let shutdown = async move {
            barrier.wait().await;
            Ok(())
        };

        handles.push(match service {
            Publisher => publisher::run(config_store.clone())
                .and_then(|_| shutdown)
                .boxed(),
            Leader { .. } => leader::run(config_store.clone())
                .and_then(|_| shutdown)
                .boxed(),
            Worker { .. } => worker::run(config_store.clone(), shutdown.map(|_| ())).boxed(),
        });
    }

    handles.try_for_each(|_| futures::future::ok(())).await?;

    Ok(())
}
