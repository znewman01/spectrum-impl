use crate::{
    config::store::Store,
    net::get_addr,
    services::discovery::{register, Group, Node, Service},
};
use log::{debug, info};

pub async fn run<C: Store>(config: &C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Leader starting up.");
    let addr = get_addr();

    let node = Node::new(Service::Leader { group: Group(0) }, addr);
    register(config, node).await?;
    debug!("Registered with config server.");

    info!("Leader shutting down.");
    Ok(())
}
