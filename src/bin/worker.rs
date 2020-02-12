use futures::prelude::*;
use spectrum_impl::{
    config, experiment,
    services::{Group, WorkerInfo},
    worker,
};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    let experiment = experiment::read_from_store(&config).await?;
    // TODO(zjn): construct from environment/args
    let info = WorkerInfo::new(Group::new(1), 1);
    worker::run(config, experiment, info, ctrl_c().map(|_| ())).await
}
