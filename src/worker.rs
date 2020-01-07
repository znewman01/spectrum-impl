use crate::config;
use futures::Future;
use log::{debug, error, info};
use tonic::{Request, Response, Status};

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{
    worker_server::{Worker, WorkerServer},
    UploadRequest, UploadResponse, VerifyRequest, VerifyResponse,
};

#[derive(Default)]
pub struct MyWorker {}

#[tonic::async_trait]
impl Worker for MyWorker {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        debug!("Request! {:?}", request.into_inner());

        let reply = UploadResponse {};
        Ok(Response::new(reply))
    }

    async fn verify(
        &self,
        _request: Request<VerifyRequest>,
    ) -> Result<Response<VerifyResponse>, Status> {
        error!("Not implemented.");
        Err(Status::new(
            tonic::Code::Unimplemented,
            "Not implemented".to_string(),
        ))
    }
}

pub async fn run<C: config::ConfigStore, F: Future<Output = ()>>(
    config_store: C,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse().unwrap();
    let server = MyWorker::default();

    // TODO: do this more async
    config_store.put(
        vec![String::from("workers"), String::from("worker")],
        String::from("[::1]:50051"),
    );
    debug!("Registered with config server.");

    tonic::transport::server::Server::builder()
        .add_service(WorkerServer::new(server))
        .serve_with_shutdown(addr, shutdown)
        .await?;

    info!("Shut down worker server.");

    Ok(())
}
