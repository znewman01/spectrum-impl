use crate::{
    config::store::Store,
    experiment,
    services::{
        discovery::{register, ServiceType},
        quorum::{set_start_time, wait_for_quorum},
    },
};
use chrono::prelude::*;
use log::{debug, info};

pub async fn run<C: Store>(config: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Publisher starting up.");

    register(&config, ServiceType::Publisher, "1").await?;
    debug!("Registered with config server.");

    let experiment = experiment::read_from_store(&config).await?;
    wait_for_quorum(&config, experiment).await?;

    let dt = DateTime::<FixedOffset>::from(Utc::now()); // TODO(zjn): should be in the future
    info!("Registering experiment start time: {}", dt);
    set_start_time(&config, dt).await?;

    info!("Publisher shutting down.");

    Ok(())
}
