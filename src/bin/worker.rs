use futures::prelude::*;
use spectrum_impl::{
    config,
    services::{Group, WorkerInfo},
    worker,
};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let info = WorkerInfo {
        group: Group(1),
        idx: 1,
    };
    worker::run(config_store, info, ctrl_c().map(|_| ())).await
}
