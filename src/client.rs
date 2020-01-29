use crate::config;
use crate::quorum::{get_addrs, wait_for_quorum, ServiceType};
use config::store::Store;
use log::{debug, info, trace};

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{worker_client::WorkerClient, ClientId, UploadRequest};

pub async fn run<C: Store>(config_store: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Client starting");
    wait_for_quorum(&config_store).await?;
    debug!("Received configuration from configuration server; initializing.");
    let mut worker_addrs: Vec<String> = get_addrs(&config_store, ServiceType::Worker).await?;
    let worker_addr = worker_addrs
        .pop()
        .ok_or("Unexpected: start time posted but no workers registered.")?;

    let mut client = WorkerClient::connect(worker_addr).await?;

    let req = tonic::Request::new(UploadRequest {
        client_id: Some(ClientId {
            client_id: "1".to_string(),
        }),
        share_and_proof: None,
    });

    trace!("About to send upload request.");
    let response = client.upload(req).await?;

    debug!("RESPONSE={:?}", response.into_inner());

    Ok(())
}
