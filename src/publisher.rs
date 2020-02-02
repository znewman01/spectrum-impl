use crate::{
    config::store::Store,
    experiment,
    net::get_addr,
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{set_start_time, wait_for_quorum},
        PublisherInfo,
    },
};

use chrono::prelude::*;
use futures::Future;
use log::{debug, error, info, trace};
use tonic::{Request, Response, Status};

pub mod spectrum {
    tonic::include_proto!("spectrum");
}

use spectrum::{
    publisher_server::{Publisher, PublisherServer},
    AggregateGroupRequest, AggregateGroupResponse,
};

#[derive(Default)]
pub struct MyPublisher {}

#[tonic::async_trait]
impl Publisher for MyPublisher {
    async fn aggregate_group(
        &self,
        _request: Request<AggregateGroupRequest>,
    ) -> Result<Response<AggregateGroupResponse>, Status> {
        error!("Not implemented.");
        Err(Status::new(
            tonic::Code::Unimplemented,
            "Not implemented".to_string(),
        ))
    }
}

pub async fn run<C, F>(
    config: C,
    info: PublisherInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Publisher starting up.");
    let addr = get_addr();
    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(PublisherServer::new(MyPublisher::default()))
            .serve_with_shutdown(addr, shutdown),
    );

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Publisher {:?} healthy and serving.", info);

    let node = Node::new(info.into(), addr);
    register(&config, node).await?;
    debug!("Registered with config server.");

    let experiment = experiment::read_from_store(&config).await?;
    wait_for_quorum(&config, experiment).await?;

    // TODO(zjn): should be more in the future
    let dt = DateTime::<FixedOffset>::from(Utc::now()) + chrono::Duration::milliseconds(100);
    info!("Registering experiment start time: {}", dt);
    set_start_time(&config, dt).await?;

    server_task.await??;
    info!("Publisher shutting down.");

    Ok(())
}
