use futures::prelude::*;
use spectrum_impl::{client, config, services::ClientInfo};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    let config_store = config::from_env()?;
    // TODO(zjn): construct from environment/args
    let info = ClientInfo::new(1);
    client::run(config_store, info, ctrl_c().map(|_| ())).await
}
