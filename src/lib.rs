//! Spectrum implementation.
use futures::future::{AbortHandle, Abortable};
use futures::prelude::*;
use log::error;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::{
    sync::{Barrier, Notify},
    time::delay_for,
};

pub mod client;
pub mod crypto;
pub mod leader;
pub mod protocols;
pub mod publisher;
pub mod worker;

pub mod bytes;
pub mod cli;
pub mod config;
pub mod experiment;
pub mod services;

mod net;

mod proto {
    use tonic::Status;

    tonic::include_proto!("spectrum");

    pub fn expect_field<T>(opt: Option<T>, name: &str) -> Result<T, Status> {
        opt.ok_or_else(|| Status::invalid_argument(format!("{} must be set.", name)))
    }
}

#[derive(fmt::Debug)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: &str) -> Error {
        Error {
            message: message.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Error::new(&error)
    }
}

impl std::error::Error for Error {}

use config::store::Store;
use experiment::Experiment;
use services::Service::{Client, Leader, Publisher, Worker};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct PublisherRemote {
    start: Arc<Notify>,
    done: Arc<Barrier>,
}

impl PublisherRemote {
    fn new(done: Arc<Barrier>, start: Arc<Notify>) -> Self {
        Self { done, start }
    }
}

#[tonic::async_trait]
impl publisher::Remote for PublisherRemote {
    async fn start(&self) {
        self.start.notify()
    }

    async fn done(&self) {
        self.done.wait().await;
    }
}

pub async fn run<C>(
    experiment: Experiment,
    config: C,
) -> Result<Duration, Box<dyn std::error::Error + Sync + Send>>
where
    C: 'static + Store + Clone + Sync + Send,
{
    experiment::write_to_store(&config, &experiment).await?;
    let started = Arc::new(Notify::new());
    // +2: +1 for the "done" notification from the publisher, +1 for the timer task
    let barrier = Arc::new(Barrier::new(
        experiment.iter_clients().count() + experiment.iter_services().count() + 2,
    ));
    let remote = PublisherRemote::new(barrier.clone(), started.clone());
    let handles = futures::stream::FuturesUnordered::new();
    for service in experiment.iter_services().chain(experiment.iter_clients()) {
        let shutdown = {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
            }
        };

        let protocol = experiment.get_protocol().clone();
        handles.push(match service {
            Publisher(info) => {
                publisher::run(config.clone(), protocol, info, remote.clone(), shutdown).boxed()
            }
            Leader(info) => {
                leader::run(config.clone(), experiment.clone(), protocol, info, shutdown).boxed()
            }
            Worker(info) => {
                worker::run(config.clone(), experiment.clone(), protocol, info, shutdown).boxed()
            }
            Client(info) => client::viewer::run(config.clone(), protocol, info, shutdown).boxed(),
        });
    }

    let timer_task = tokio::spawn(async move {
        started.notified().await;
        let start_time = Instant::now();
        barrier.wait().await;
        start_time.elapsed()
    });
    let delay_task = tokio::spawn(delay_for(TIMEOUT));
    let (work, abort_rx) = AbortHandle::new_pair();
    tokio::spawn(Abortable::new(
        async move {
            handles
                .for_each(|result| async {
                    result.unwrap_or_else(|err| error!("Task resulted in error: {:?}", err));
                })
                .await
        },
        abort_rx,
    ));

    futures::select! {
        elapsed = timer_task.fuse() => Ok(elapsed?),
        _ = delay_task.fuse() => {
            work.abort();
            Err(Box::new(Error::new(format!("Task timed out after {:?}.", TIMEOUT).as_str())))
        }
    }
}
