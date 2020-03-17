use crate::proto::{
    expect_field,
    publisher_server::{Publisher, PublisherServer},
    AggregateGroupRequest, AggregateGroupResponse,
};
use crate::{
    config::store::Store,
    experiment,
    experiment::Experiment,
    net::get_addr,
    protocols::{accumulator::Accumulator, Bytes},
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
use std::sync::Arc;
use tokio::spawn;
use tonic::{Request, Response, Status};

pub struct MyPublisher {
    accumulator: Arc<Accumulator<Vec<Bytes>>>,
    total_groups: usize,
}

impl MyPublisher {
    fn from_experiment(experiment: Experiment) -> Self {
        MyPublisher {
            accumulator: Arc::new(Accumulator::new(vec![
                Default::default();
                experiment.channels
            ])),
            total_groups: experiment.groups as usize,
        }
    }
}

#[tonic::async_trait]
impl Publisher for MyPublisher {
    async fn aggregate_group(
        &self,
        request: Request<AggregateGroupRequest>,
    ) -> Result<Response<AggregateGroupResponse>, Status> {
        let request = request.into_inner();
        trace!("Request! {:?}", request);

        let data = expect_field(request.share, "Share")?.data;
        let total_groups = self.total_groups;
        let accumulator = self.accumulator.clone();

        // TODO: factor out?
        spawn(async move {
            // TODO: spawn_blocking for heavy computation?
            let data: Vec<Bytes> = data.into_iter().map(Into::into).collect();
            let group_count = accumulator.accumulate(data).await;
            if group_count < total_groups {
                trace!(
                    "Publisher receieved {}/{} shares",
                    group_count,
                    total_groups
                );
                return;
            }
            if group_count > total_groups {
                error!(
                    "Too many shares recieved! Got {}, expected {}",
                    group_count, total_groups
                );
                return;
            }

            let share = accumulator.get().await;
            info!("Publisher final shares: {:?}", share);
        });

        Ok(Response::new(AggregateGroupResponse {}))
    }
}

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    info: PublisherInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    let state = MyPublisher::from_experiment(experiment);
    info!("Publisher starting up.");
    let addr = get_addr();
    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(PublisherServer::new(state))
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
    let dt = DateTime::<FixedOffset>::from(Utc::now()) + chrono::Duration::milliseconds(1000);
    info!("Registering experiment start time: {}", dt);
    set_start_time(&config, dt).await?;

    server_task.await??;
    info!("Publisher shutting down.");

    Ok(())
}
