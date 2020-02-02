use crate::{
    config::store::Store,
    net::get_addr,
    services::discovery::{register, Node, WorkerInfo},
    services::health::{wait_for_health, AllGoodHealthServer, HealthServer},
};
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

pub async fn run<C, F>(
    config_store: C,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(WorkerServer::new(MyWorker::default()))
            .serve_with_shutdown(addr, shutdown),
    );

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Worker healthy.");

    let node = Node::new(info.into(), addr);
    register(&config_store, node).await?;
    debug!("Registered with config server.");

    server_task.await??;
    info!("Worker shutting down.");

    Ok(())
}
