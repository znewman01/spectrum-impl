use crate::{
    config::store::Store,
    net::get_addr,
    services::discovery::{register, Node, Service},
};
use log::{debug, info};

pub async fn run<C: Store>(
    config: C,
    service: Service,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>> {
    match service {
        Service::Leader { .. } => {}
        _ => panic!("Invalid leader config."),
    };

    info!("Leader starting up.");
    let addr = get_addr();

    let node = Node::new(service, addr);
    register(&config, node).await?;
    debug!("Registered with config server.");

    info!("Leader shutting down.");
    Ok(())
}
