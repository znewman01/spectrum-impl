use futures::prelude::*;
use spectrum_impl::{
    config, experiment, leader,
    services::{Group, LeaderInfo},
};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env().await?;
    let experiment = experiment::read_from_store(&config).await?;
    // TODO(zjn): construct from environment/args
    let info = LeaderInfo::new(Group::new(1));
    let protocol = experiment.get_protocol().clone();
    leader::run(config, experiment, protocol, info, ctrl_c().map(|_| ())).await
}
