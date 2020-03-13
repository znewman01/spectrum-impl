use crate::proto::{
    leader_server::{Leader, LeaderServer},
    AggregateWorkerRequest, AggregateWorkerResponse,
};
use crate::{
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::accumulator::Accumulator,
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        LeaderInfo,
    },
};

use futures::Future;
use log::{debug, error, info, trace};
use std::sync::Arc;
use tokio::spawn;
use tonic::{Request, Response, Status};

pub struct MyLeader {
    accumulator: Arc<Accumulator<Vec<u8>>>,
    total_workers: usize,
}

impl MyLeader {
    fn from_experiment(experiment: Experiment) -> Self {
        MyLeader {
            accumulator: Arc::new(Accumulator::new(vec![0u8; experiment.channels])),
            total_workers: experiment.workers_per_group as usize,
        }
    }
}

// TODO factor out
fn expect_field<T>(opt: Option<T>, name: &str) -> Result<T, Status> {
    opt.ok_or_else(|| Status::invalid_argument(format!("{} must be set.", name)))
}

#[tonic::async_trait]
impl Leader for MyLeader {
    async fn aggregate_worker(
        &self,
        request: Request<AggregateWorkerRequest>,
    ) -> Result<Response<AggregateWorkerResponse>, Status> {
        let request = request.into_inner();
        trace!("Request! {:?}", request);

        let data: Vec<u8> = expect_field(request.share, "Share")?
            .data
            .iter()
            .map(|x| x[0])
            .collect();
        let accumulator = self.accumulator.clone();
        let total_workers = self.total_workers;

        spawn(async move {
            // TODO: spawn_blocking for heavy computation?
            let worker_count = accumulator.accumulate(data).await;
            if worker_count < total_workers {
                trace!("Leader receieved {}/{} shares", worker_count, total_workers);
                return;
            }
            if worker_count > total_workers {
                error!(
                    "Too many shares recieved! Got {}, expected {}",
                    worker_count, total_workers
                );
                return;
            }

            debug!("Leader final shares: {:?}", accumulator.get().await);
            info!("Should forward to publisher.");
        });

        Ok(Response::new(AggregateWorkerResponse {}))
    }
}

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    info: LeaderInfo,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    let state = MyLeader::from_experiment(experiment);
    info!("Leader starting up.");
    let addr = get_addr();
    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(LeaderServer::new(state))
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
