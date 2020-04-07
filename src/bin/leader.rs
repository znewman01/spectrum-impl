use clap::{crate_authors, crate_version, Clap};
use futures::prelude::*;
use spectrum_impl::{
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
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Args {
    #[clap(flatten)]
    logs: cli::LogArgs,
    #[clap(flatten)]
    leader: LeaderArgs,
}

#[derive(Clap)]
struct LeaderArgs {
    /// The index of the group of this leader.
    #[clap(long, env = "SPECTURM_LEADER_GROUP")]
    group: u16,
}

impl From<LeaderArgs> for LeaderInfo {
    fn from(args: LeaderArgs) -> Self {
        Self::new(Group::new(args.group))
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
    leader::run(config, experiment, protocol, info, ctrl_c().map(|_| ())).await
}
