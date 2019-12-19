use crate::config;
use std::time::Duration;
use std::rc::Rc;

pub mod prototest {
    tonic::include_proto!("prototest");
}

use prototest::{client::ServerClient, Ping};

pub async fn run(config_store: Rc<dyn config::ConfigStore>) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        if config_store.list(vec![String::from("servers")]).len() > 0 {
            // shouldn't need to sleep here but server does stuff sync and weird
            tokio::timer::delay_for(Duration::from_millis(200)).await;
            break;
        }
        tokio::timer::delay_for(Duration::from_secs(2)).await; // hack; should use retries
    }

    println!("client starting");
    let mut client = ServerClient::connect("http://[::1]:50051").await?;

    let req = tonic::Request::new(Ping {});

    let response = client.ping_pong(req).await?;

    println!("RESPONSE={:?}", response);

    Ok(())
}
