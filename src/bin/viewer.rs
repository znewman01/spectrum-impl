use futures::prelude::*;
use spectrum_impl::{client, config, experiment, services::ClientInfo};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config = config::from_env()?;
    let experiment = experiment::read_from_store(&config).await?;
    // TODO(zjn): construct from environment/args
    let info = ClientInfo::new(1);
    client::viewer::run(config, experiment, info, ctrl_c().map(|_| ())).await
}
