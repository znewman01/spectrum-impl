use std::time::Duration;

pub mod prototest {
    tonic::include_proto!("prototest");
}

use prototest::{client::ServerClient, Ping};

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tokio::timer::delay_for(Duration::from_secs(1)).await; // hack; should use retries
    println!("client starting");
    let mut client = ServerClient::connect("http://[::1]:50051").await?;

    let req = tonic::Request::new(Ping {});

    let response = client.ping_pong(req).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
