//! Spectrum implementation.
use futures::prelude::*;
use log::error;
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
use services::Service::{Leader, Publisher, Worker};

pub async fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    let experiment = Experiment::new(2, 2, 2);
    experiment::write_to_store(&config, experiment).await?;
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
        handles.push(client::run(config.clone()).and_then(|_| shutdown).boxed());
    }

    for service in experiment.iter_services() {
        let barrier = barrier.clone();
        let shutdown = async move {
            barrier.wait().await;
        };

        handles.push(match service {
            Publisher(info) => publisher::run(config.clone(), info, shutdown).boxed(),
            Leader(info) => leader::run(config.clone(), info, shutdown).boxed(),
            Worker(info) => worker::run(config.clone(), info, shutdown).boxed(),
        });
    }

    handles
        .for_each(|result| async {
            result.unwrap_or_else(|err| error!("Task resulted in error: {:?}", err));
        })
        .await;

    Ok(())
}
