use crate::proto::{
    leader_server::{Leader, LeaderServer},
    AggregateWorkerRequest, AggregateWorkerResponse,
};
use crate::{
    config::store::Store,
    net::get_addr,
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        LeaderInfo,
    },
};

use futures::Future;
use log::{debug, error, info, trace};
use tonic::{Request, Response, Status};

#[derive(Default)]
pub struct MyLeader {}

#[tonic::async_trait]
impl Leader for MyLeader {
    async fn aggregate_worker(
        &self,
        _request: Request<AggregateWorkerRequest>,
    ) -> Result<Response<AggregateWorkerResponse>, Status> {
        error!("Not implemented.");
        Err(Status::new(
            tonic::Code::Unimplemented,
            "Not implemented".to_string(),
        ))
    }
}

pub async fn run<C, F>(
    config: C,
    info: LeaderInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Leader starting up.");
    let addr = get_addr();
    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(LeaderServer::new(MyLeader::default()))
            .serve_with_shutdown(addr, shutdown),
    );

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Leader {:?} healthy and serving.", info);

    let node = Node::new(info.into(), addr);
    register(&config, node).await?;
    debug!("Registered with config server.");

    server_task.await??;
    info!("Leader shutting down.");
    Ok(())
}
