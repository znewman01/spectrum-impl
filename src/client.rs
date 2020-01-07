use crate::config;
use std::time::Duration;

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{worker_client::WorkerClient, ClientId, UploadRequest};

pub async fn run<C: config::ConfigStore>(
    config_store: C,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        if !config_store.list(vec![String::from("workers")]).is_empty() {
            // shouldn't need to sleep here but worker does stuff sync and weird
            tokio::time::delay_for(Duration::from_millis(200)).await;
            break;
        }
        tokio::time::delay_for(Duration::from_secs(2)).await; // hack; should use retries
    }

    println!("client starting");
    let mut client = WorkerClient::connect("http://[::1]:50051").await?;

    let req = tonic::Request::new(UploadRequest {
        client_id: Some(ClientId {
            client_id: "1".to_string(),
        }),
        share_and_proof: None,
    });

    let response = client.upload(req).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
