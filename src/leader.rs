use crate::{
    config::store::Store,
    net::get_addr,
    services::discovery::{register, LeaderInfo, Node},
};
use log::{debug, info};

pub async fn run<C: Store>(
    config: C,
    info: LeaderInfo,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    info!("Leader starting up.");
    let addr = get_addr();

    let node = Node::new(info.into(), addr);
    register(&config, node).await?;
    debug!("Registered with config server.");

    info!("Leader shutting down.");
    Ok(())
}
