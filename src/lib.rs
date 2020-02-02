//! Spectrum implementation.
use futures::{FutureExt, TryFutureExt, TryStreamExt};
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
pub mod services;

use experiment::Experiment;
use services::discovery::Service::{Leader, Publisher, Worker};

pub async fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    let experiment = Experiment::new(2, 2, 5);
    experiment::write_to_store(&config_store, experiment).await?;
    let barrier = Arc::new(Barrier::new(
        experiment.clients as usize + experiment.iter_services().count(),
    ));
    let handles = futures::stream::FuturesUnordered::new();

    for _ in 0..experiment.clients {
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
            Publisher(info) => publisher::run(config_store.clone(), info)
                .and_then(|_| shutdown)
                .boxed(),
            Leader(info) => leader::run(config_store.clone(), info)
                .and_then(|_| shutdown)
                .boxed(),
            Worker(info) => worker::run(config_store.clone(), info, shutdown.map(|_| ())).boxed(),
        });
    }

    handles.try_for_each(|_| futures::future::ok(())).await?;

    Ok(())
}
