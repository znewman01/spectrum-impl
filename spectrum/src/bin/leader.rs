use clap::{crate_authors, crate_version, Parser};
use futures::prelude::*;
use spectrum::{
    cli, config, experiment, leader,
    services::{Group, LeaderInfo},
};
use tokio::signal::ctrl_c;

/// Run a Spectrum leader (one per trust group).
///
/// The leader is responsible for aggregating shares *within* trust groups;
/// it receives shares from each worker in the group, and forwards them to the
/// publisher.
///
/// Use `$SPECTRUM_CONFIG_SERVER=etcd://127.0.0.1:8000` to point to an etcd
/// instance, and the leader will pick up the experiment configuration from
/// there.
#[derive(Parser)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    logs: cli::LogArgs,
    #[clap(flatten)]
    leader: LeaderArgs,
    #[clap(flatten)]
    net: cli::NetArgs,
}

#[derive(Parser)]
struct LeaderArgs {
    /// The index of the group of this leader.
    #[clap(long, env = "SPECTRUM_LEADER_GROUP")]
    group: u16,
}

impl From<LeaderArgs> for LeaderInfo {
    fn from(args: LeaderArgs) -> Self {
        // -1 because the CLI needs non-zero or it thinks we didn't supply it
        // from environment variable
        Self::new(Group::new(args.group - 1))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let args = Args::parse();
    args.logs.init();

    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    let protocol = experiment.get_protocol().clone();
    let info = LeaderInfo::from(args.leader);
    leader::run(
        config,
        experiment,
        protocol,
        info,
        args.net.into(),
        ctrl_c().map(|_| ()),
    )
    .await
}
