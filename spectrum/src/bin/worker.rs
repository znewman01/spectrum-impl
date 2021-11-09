use clap::{crate_authors, crate_version, Parser};
use futures::prelude::*;
use spectrum::{
    cli, config, experiment,
    services::{Group, WorkerInfo},
    worker,
};
use tokio::signal::ctrl_c;

/// Run a Spectrum worker (many per trust group).
///
/// Clients connect directly to workers (sharded within each group).
/// The workers receive shares, validate them (with their peers in *other* trust
/// groups), aggregate them, and forward the aggregated shares to their group
/// leader.
///
/// Use `$SPECTRUM_CONFIG_SERVER=etcd://127.0.0.1:8000` to point to an etcd
/// instance, and the worker will pick up the experiment configuration from
/// there.
#[derive(Parser)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    logs: cli::LogArgs,
    #[clap(flatten)]
    worker: WorkerArgs,
    #[clap(flatten)]
    net: cli::NetArgs,
}

#[derive(Parser)]
struct WorkerArgs {
    /// The index of the group of this worker.
    #[clap(long, env = "SPECTRUM_WORKER_GROUP")]
    group: u16,

    /// The index within the group of this worker.
    #[clap(long = "index", env = "SPECTRUM_WORKER_INDEX")]
    idx: u16,
}

impl From<WorkerArgs> for WorkerInfo {
    fn from(args: WorkerArgs) -> Self {
        // -1 because the CLI needs non-zero or it thinks we didn't supply it
        // from environment variable
        Self::new(Group::new(args.group - 1), args.idx - 1)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let protocol = experiment.get_protocol().clone();
    let info = WorkerInfo::from(args.worker);
    worker::run(
        config,
        experiment,
        protocol,
        info,
        args.net.into(),
        ctrl_c().map(|_| ()),
    )
    .await
}
