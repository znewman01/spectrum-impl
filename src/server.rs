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

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let server = MyServer::default();

    tonic::transport::server::Server::builder()
        .add_service(ServerServer::new(server))
        .serve(addr)
        .await?;

    Ok(())
}
