use crate::config;
use std::sync::Arc;
use tonic::{Request, Response, Status};

pub mod prototest {
    tonic::include_proto!("prototest");
}

use prototest::{
    server::{Server, ServerServer},
    Ping, Pong,
};

#[derive(Default)]
pub struct MyServer {}

#[tonic::async_trait]
impl Server for MyServer {
    async fn ping_pong(&self, request: Request<Ping>) -> Result<Response<Pong>, Status> {
        println!("Request! {:?}", request);

        let reply = Pong {};
        Ok(Response::new(reply))
    }
}

pub async fn run(
    config_store: Arc<dyn config::ConfigStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let server = MyServer::default();

    // TODO: do this more async
    config_store.put(
        vec![String::from("servers"), String::from("server")],
        String::from("[::1]:50051"),
    );

    tonic::transport::server::Server::builder()
        .add_service(ServerServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}
