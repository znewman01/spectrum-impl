use crate::config;
use crate::health::{wait_for_health, AllGoodHealthServer, HealthServer};
use crate::quorum;
use chrono::prelude::*;
use config::store::Store;
use futures::Future;
use log::{debug, error, info, trace};
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

pub async fn run<C, F>(config_store: C, shutdown: F) -> Result<(), Box<dyn std::error::Error>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = "127.0.0.1:50051"; // TODO(zjn): use IPv6 if available

    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(WorkerServer::new(MyWorker::default()))
            .serve_with_shutdown(addr.parse()?, shutdown),
    );

    let url = format!("http://{}", addr);
    wait_for_health(url).await?;
    trace!("Worker healthy.");

    let dt = DateTime::<FixedOffset>::from(Utc::now()); // TODO(zjn): should be in the future
    quorum::set_quorum(&config_store, dt).await?;
    debug!("Registered with config server.");

    server_task.await??;
    info!("Worker shutting down.");

    Ok(())
}
