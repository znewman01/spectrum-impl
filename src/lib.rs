//! Spectrum implementation.
use futures::prelude::*;
use log::error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;

pub mod client;
pub mod crypto;
pub mod leader;
pub mod protocols;
pub mod publisher;
pub mod worker;

pub mod bytes;
pub mod config;
pub mod experiment;
mod net;
pub mod services;
mod proto {
    use tonic::Status;

    tonic::include_proto!("spectrum");

    pub fn expect_field<T>(opt: Option<T>, name: &str) -> Result<T, Status> {
        opt.ok_or_else(|| Status::invalid_argument(format!("{} must be set.", name)))
    }
}

use experiment::Experiment;
use services::Service::{Client, Leader, Publisher, Worker};

const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone)]
struct PublisherRemote {
    done: Arc<Barrier>,
}

impl PublisherRemote {
    fn new(done: Arc<Barrier>) -> Self {
        Self { done }
    }
}

#[tonic::async_trait]
impl publisher::Remote for PublisherRemote {
    async fn done(&self) {
        self.done.wait().await;
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    let experiment = Experiment::new(2, 2, 10, 2);
    experiment::write_to_store(&config, experiment).await?;
    // TODO: +1 for the "done" notification from the publisher
    let barrier = Arc::new(Barrier::new(
        experiment.iter_clients().count() + experiment.iter_services().count() + 1,
    ));
    let remote = PublisherRemote::new(barrier.clone());
    let handles = futures::stream::FuturesUnordered::new();
    for service in experiment.iter_services().chain(experiment.iter_clients()) {
        let shutdown = {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
            }
        };

        let protocol = experiment.get_protocol();
        handles.push(match service {
            Publisher(info) => {
                publisher::run(config.clone(), protocol, info, remote.clone(), shutdown).boxed()
            }
            Leader(info) => {
                leader::run(config.clone(), experiment, protocol, info, shutdown).boxed()
            }
            Worker(info) => {
                worker::run(config.clone(), experiment, protocol, info, shutdown).boxed()
            }
            Client(info) => client::viewer::run(config.clone(), protocol, info, shutdown).boxed(),
        });
    }

    // TODO: timer task
    // - wait for start time notification
    // - wait for done notification
    // - return the difference
    // - if takes too long (TIMEOUT), kill everything -- need something in shutdown?

    handles
        .for_each(|result| async {
            result.unwrap_or_else(|err| error!("Task resulted in error: {:?}", err));
        })
        .await;

    Ok(())
}
