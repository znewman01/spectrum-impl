use crate::{
    config::store::Store,
    services::discovery::{register, ServiceType},
};
use log::{debug, info};

pub async fn run<C: Store>(config: &C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Leader starting up.");

    register(config, ServiceType::Leader, "1").await?;
    debug!("Registered with config server.");

    info!("Leader shutting down.");
    Ok(())
}
