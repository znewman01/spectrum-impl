use crate::config::store::Store;
use crate::quorum::{get_addrs, set_quorum, ServiceType};
use chrono::prelude::*;
use log::info;
use std::time::Duration;
use tokio::time::delay_for;

pub async fn run<C: Store>(config: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Publisher starting up.");

    // TODO(zjn): refactor into quorum library
    loop {
        if !get_addrs(&config, ServiceType::Worker).await?.is_empty() {
            info!("Detected quorum.");
            break;
        }
        delay_for(Duration::from_millis(50)).await;
    }

    let dt = DateTime::<FixedOffset>::from(Utc::now()); // TODO(zjn): should be in the future
    info!("Registering experiment start time: {}", dt);
    set_quorum(&config, dt).await?;

    info!("Publisher shutting down.");

    Ok(())
}
