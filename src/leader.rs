use crate::{
    config::store::Store,
    services::discovery::{register, Group, Node, Service},
};
use log::{debug, info};
use std::net::SocketAddr;

pub async fn run<C: Store>(config: &C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Leader starting up.");
    let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), 50052);

    let node = Node::new(Service::Leader { group: Group(0) }, addr);
    register(config, node).await?;
    debug!("Registered with config server.");

    info!("Leader shutting down.");
    Ok(())
}
