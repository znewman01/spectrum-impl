use crate::{
    config,
    services::{
        discovery::{resolve_all, Service},
        quorum::wait_for_start_time_set,
    },
};
use config::store::Store;
use log::{debug, info, trace};

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{worker_client::WorkerClient, ClientId, UploadRequest};

pub async fn run<C: Store>(config_store: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Client starting");
    wait_for_start_time_set(&config_store).await?;
    debug!("Received configuration from configuration server; initializing.");

    let worker_addr = resolve_all(&config_store)
        .await?
        .iter()
        .find(|node| match node.service {
            Service::Worker { .. } => true,
            _ => false,
        })
        .unwrap()
        .addr
        .to_string();
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
