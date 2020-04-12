//! Spectrum implementation.
use futures::{
    future::{AbortHandle, Abortable},
    prelude::*,
    stream::FuturesUnordered,
};
use log::error;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::{
    process::Command,
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
pub mod net;
pub mod services;

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

pub async fn run_in_process<C>(
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
    let handles = FuturesUnordered::new();
    for service in experiment.iter_services().chain(experiment.iter_clients()) {
        let shutdown = {
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
            }
        };

        let protocol = experiment.get_protocol().clone();
        let net = net::Config::with_free_port_localhost();
        handles.push(match service {
            Publisher(info) => publisher::run(
                config.clone(),
                protocol,
                info,
                net,
                remote.clone(),
                shutdown,
            )
            .boxed(),
            Leader(info) => leader::run(
                config.clone(),
                experiment.clone(),
                protocol,
                info,
                net,
                shutdown,
            )
            .boxed(),
            Worker(info) => worker::run(
                config.clone(),
                experiment.clone(),
                protocol,
                info,
                net,
                shutdown,
            )
            .boxed(),
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

pub async fn run_new_processes<C>(
    experiment: Experiment,
    config: C,
    etcd_env: (String, String),
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: 'static + Store + Clone + Sync + Send,
{
    experiment::write_to_store(&config, &experiment).await?;

    let data_dir = tempfile::tempdir()?;
    let bin_dir = env::var_os("SPECTRUM_BIN_DIR").ok_or("Must set SPECTRUM_BIN_DIR")?;
    let bin_dir = Path::new(&bin_dir);
    let mut publisher_handle = None;
    let mut handles = vec![];
    for service in experiment.iter_services().chain(experiment.iter_clients()) {
        match service {
            Publisher(_) => {
                // TODO: publisher stdout should be the time we care about
                publisher_handle.replace(
                    Command::new(bin_dir.join("publisher"))
                        .args(&["--log-level", "info"])
                        .env(&etcd_env.0, &etcd_env.1)
                        .spawn()?,
                );
            }
            Leader(info) => {
                handles.push(
                    Command::new(bin_dir.join("leader"))
                        .args(&["--log-level", "info"])
                        .args(&["--group", &(info.group.idx + 1).to_string()])
                        .env(&etcd_env.0, &etcd_env.1)
                        .spawn()?,
                );
            }
            Worker(info) => {
                handles.push(
                    Command::new(bin_dir.join("worker"))
                        .args(&["--log-level", "info"])
                        .args(&["--group", &(info.group.idx + 1).to_string()])
                        .args(&["--index", &(info.idx + 1).to_string()])
                        .env(&etcd_env.0, &etcd_env.1)
                        .spawn()?,
                );
            }
            Client(info) => match info.broadcast {
                Some((msg, key)) => {
                    let key_file = data_dir.path().join(format!("key-{}.json", info.idx));
                    serde_json::to_writer(File::create(&key_file)?, &key)?;

                    let msg_file = data_dir.path().join(format!("msg-{}.json", info.idx));
                    File::create(&msg_file)?.write_all(msg.as_ref())?;
                    handles.push(
                        Command::new(bin_dir.join("broadcaster"))
                            .args(&["--log-level", "info"])
                            .args(&["--index", &(info.idx + 1).to_string()])
                            .args(&["--key-file", &key_file.to_string_lossy()])
                            .args(&["--message-file", &msg_file.to_string_lossy()])
                            .env(&etcd_env.0, &etcd_env.1)
                            .spawn()?,
                    );
                }
                None => {
                    handles.push(
                        Command::new(bin_dir.join("viewer"))
                            .args(&["--log-level", "warn"])
                            .args(&["--index", &(info.idx + 1).to_string()])
                            .env(&etcd_env.0, &etcd_env.1)
                            .spawn()?,
                    );
                }
            },
        }
    }

    // TODO: kill everybody on ^C
    publisher_handle
        .expect("Must have at least one publisher in the experiment.")
        .await?;
    // TODO: should:
    // - kill (probably just killing at first is okay too)
    // TOOD:
    // - send ^C to non-publisher processes first?
    //   https://stackoverflow.com/questions/49210815/how-do-i-send-a-signal-to-a-child-subprocess
    // - then wait a little while

    Ok(())
}
