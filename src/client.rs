use crate::config;
use config::store::Store;
use log::{debug, info, trace};
use std::time::Duration;

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{worker_client::WorkerClient, ClientId, UploadRequest};

pub async fn run<C: Store>(config_store: C) -> Result<(), Box<dyn std::error::Error>> {
    info!("Client starting");
    loop {
        if !config_store
            .list(vec![String::from("workers")])
            .await?
            .is_empty()
        {
            // shouldn't need to sleep here but worker does stuff sync and weird
            tokio::time::delay_for(Duration::from_millis(100)).await;
            break;
        }
        tokio::time::delay_for(Duration::from_millis(100)).await; // hack; should use retries
    }
    debug!("Received configuration from configuration server; initializing.");

    let worker_addr = "http://127.0.0.1:50051"; // TODO(zjn): get from config server
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
