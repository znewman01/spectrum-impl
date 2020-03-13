use crate::proto::{
    leader_server::{Leader, LeaderServer},
    publisher_client::PublisherClient,
    AggregateGroupRequest, AggregateWorkerRequest, AggregateWorkerResponse, Share,
};
use crate::{
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::accumulator::Accumulator,
    services::{
        discovery::{register, resolve_all, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::wait_for_start_time_set,
        LeaderInfo, Service,
    },
};

use futures::Future;
use log::{debug, error, info, trace};
use std::sync::Arc;
use tokio::{
    spawn,
    sync::{watch, Mutex},
};
use tonic::{transport::Channel, Request, Response, Status};

type SharedPublisherClient = Arc<Mutex<PublisherClient<Channel>>>;

pub struct MyLeader {
    accumulator: Arc<Accumulator<Vec<u8>>>,
    total_workers: usize,
    publisher_client: watch::Receiver<Option<SharedPublisherClient>>,
}

impl MyLeader {
    fn from_experiment(
        experiment: Experiment,
        publisher_client: watch::Receiver<Option<SharedPublisherClient>>,
    ) -> Self {
        MyLeader {
            accumulator: Arc::new(Accumulator::new(vec![0u8; experiment.channels])),
            total_workers: experiment.workers_per_group as usize,
            publisher_client,
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
        let publisher = self
            .publisher_client
            .borrow()
            .as_ref()
            .expect("Should have a publisher by now.")
            .clone();

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

            let share = accumulator.get().await;
            debug!("Leader final shares: {:?}", share);
            let req = Request::new(AggregateGroupRequest {
                share: Some(Share {
                    data: share.into_iter().map(|x| vec![x]).collect(),
                }),
            });
            publisher.lock().await.aggregate_group(req).await.unwrap();
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
    let (tx, rx) = watch::channel(None);
    let state = MyLeader::from_experiment(experiment, rx);
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

    wait_for_start_time_set(&config).await.unwrap();
    let publisher_addr = resolve_all(&config)
        .await?
        .iter()
        .find_map(|node| match node.service {
            Service::Publisher(_) => Some(node.addr),
            _ => None,
        })
        .expect("Should have a publisher registered");

    let publisher = Arc::new(Mutex::new(
        PublisherClient::connect(format!("http://{}", publisher_addr)).await?,
    ));
    tx.broadcast(Some(publisher))
        .or_else(|_| Err("Error sending service registry."))?;

    server_task.await??;
    info!("Leader shutting down.");
    Ok(())
}
