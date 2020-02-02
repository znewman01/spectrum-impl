use futures::prelude::*;
use spectrum_impl::{
    config,
    services::discovery::{Group, Service},
    worker,
};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let service = Service::Worker {
        group: Group(1),
        idx: 1,
    };
    worker::run(config_store, service, ctrl_c().map(|_| ())).await
}
