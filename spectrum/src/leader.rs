use crate::proto::{
    expect_field,
    leader_server::{Leader, LeaderServer},
    publisher_client::PublisherClient,
    AggregateGroupRequest, AggregateWorkerRequest, AggregateWorkerResponse, Share,
};
use crate::{
    bytes::Bytes,
    config::store::Store,
    experiment::Experiment,
    net::Config as NetConfig,
    protocols::{accumulator::Accumulator, wrapper::ProtocolWrapper, Protocol},
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

pub struct MyLeader<P: Protocol> {
    accumulator: Arc<Accumulator<Vec<P::Accumulator>>>,
    total_workers: usize,
    publisher_client: watch::Receiver<Option<SharedPublisherClient>>,
}

impl<P> MyLeader<P>
where
    P: Protocol,
    P::Accumulator: Clone,
{
    fn from_protocol(
        protocol: P,
        workers_per_group: u16,
        publisher_client: watch::Receiver<Option<SharedPublisherClient>>,
    ) -> Self {
        MyLeader {
            accumulator: Arc::new(Accumulator::new(protocol.new_accumulator())),
            total_workers: workers_per_group as usize,
            publisher_client,
        }
    }
}

#[tonic::async_trait]
impl<P> Leader for MyLeader<P>
where
    P: Protocol + 'static,
    P::Accumulator: Clone + Sync + Send + From<Bytes>,
{
    async fn aggregate_worker(
        &self,
        request: Request<AggregateWorkerRequest>,
    ) -> Result<Response<AggregateWorkerResponse>, Status> {
        let request = request.into_inner();

        let data = expect_field(request.share, "Share")?.data;
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
            let data: Vec<_> = data
                .into_iter()
                .map(Bytes::from)
                .map(P::Accumulator::from)
                .collect();
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
            let share: Vec<Bytes> = share.into_iter().map(Into::into).collect();
            let share: Vec<Vec<u8>> = share.into_iter().map(Into::into).collect();
            // trace!("Leader final shares: {:?}", share);
            let req = Request::new(AggregateGroupRequest {
                share: Some(Share { data: share }),
            });
            publisher.lock().await.aggregate_group(req).await.unwrap();
        });

        Ok(Response::new(AggregateWorkerResponse {}))
    }
}

async fn inner_run<C, F, P>(
    config: C,
    experiment: Experiment,
    protocol: P,
    info: LeaderInfo,
    net: NetConfig,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
    P: Protocol + 'static,
    P::Accumulator: Sync + Send + Clone + From<Bytes>,
{
    let (tx, rx) = watch::channel(None);
    let state = MyLeader::from_protocol(protocol, experiment.group_size(), rx);
    info!("Leader starting up.");
    let server_task = tokio::spawn(
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(LeaderServer::new(state))
            .serve_with_shutdown(net.local_socket_addr(), shutdown),
    );

    wait_for_health(format!("http://{}", net.public_addr())).await?;
    trace!("Leader {:?} healthy and serving.", info);

    let node = Node::new(info.into(), net.public_addr());
    register(&config, node).await?;
    debug!("Registered with config server.");

    wait_for_start_time_set(&config).await.unwrap();
    let publisher_addr = resolve_all(&config)
        .await?
        .into_iter()
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

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    protocol: ProtocolWrapper,
    info: LeaderInfo,
    net: NetConfig,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper::Secure(protocol) => {
            inner_run(config, experiment, protocol, info, net, shutdown).await?;
        }
        ProtocolWrapper::Insecure(protocol) => {
            inner_run(config, experiment, protocol, info, net, shutdown).await?;
        }
    }
    Ok(())
}
