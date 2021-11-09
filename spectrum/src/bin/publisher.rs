use clap::{crate_authors, crate_version, Parser};
use futures::prelude::*;
use spectrum::{cli, config, experiment, publisher, services::PublisherInfo};
use std::sync::Arc;
use std::time::Instant;
use tokio::signal::ctrl_c;
use tokio::sync::{Mutex, Notify};

/// Run a Spectrum publisher (one per deployment).
///
/// The publisher is responsible for aggregating shares *between* trust groups;
/// it receives shares from the leader of each group.
///
/// Use `$SPECTRUM_CONFIG_SERVER=etcd://127.0.0.1:8000` to point to an etcd
/// instance, and the publisher will pick up the experiment configuration from
/// there.
#[derive(Parser)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    logs: cli::LogArgs,
    #[clap(flatten)]
    net: cli::NetArgs,
    /// How long to delay between quorum and clients start.
    ///
    /// Might need to increase this if lots of clients on the same machine.
    #[clap(long, env = "SPECTRUM_DELAY_MS", default_value = "5000")]
    delay_ms: i64,
}

#[derive(Debug, Clone)]
struct CliRemote {
    start: Arc<Mutex<Option<Instant>>>,
    done: Arc<Notify>,
}

impl CliRemote {
    fn new(done: Arc<Notify>) -> Self {
        CliRemote {
            start: Default::default(),
            done,
        }
    }
}

#[tonic::async_trait]
impl publisher::Remote for CliRemote {
    async fn start(&self) {
        let mut start = self.start.lock().await;
        start.replace(Instant::now());
    }

    async fn done(&self) {
        let start = self.start.lock().await;
        let elapsed = start.expect("Can't call done() before start()!").elapsed();
        eprintln!("Elapsed time: {}ms", elapsed.as_millis());
        self.done.notify_one();
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let info = PublisherInfo::new();

    let done = Arc::new(Notify::new());
    let remote = CliRemote::new(done.clone());
    let shutdown = async move {
        futures::select! {
            _ = ctrl_c().fuse() => {},
            _ = done.notified().fuse() => {},
        }
    };

    publisher::run(
        config,
        experiment.get_protocol().clone(),
        info,
        args.net.into(),
        remote,
        shutdown,
        args.delay_ms,
    )
    .await
}
