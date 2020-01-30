use crate::{
    config::store::Store,
    services::quorum::{set_start_time, wait_for_quorum},
};
use chrono::prelude::*;
use log::info;

pub async fn run<C: Store>(config: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Publisher starting up.");

    wait_for_quorum(&config).await?;

    let dt = DateTime::<FixedOffset>::from(Utc::now()); // TODO(zjn): should be in the future
    info!("Registering experiment start time: {}", dt);
    set_start_time(&config, dt).await?;

    info!("Publisher shutting down.");

    Ok(())
}
