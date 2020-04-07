use crate::proto::{
    expect_field,
    publisher_server::{Publisher, PublisherServer},
    AggregateGroupRequest, AggregateGroupResponse,
};
use crate::{
    bytes::Bytes,
    config::store::Store,
    experiment,
    net::Config as NetConfig,
    protocols::{accumulator::Accumulator, wrapper::ProtocolWrapper, Protocol},
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, set_start_time, wait_for_quorum},
        PublisherInfo,
    },
};

use chrono::prelude::*;
use futures::prelude::*;
use log::{debug, error, info, trace};
use std::sync::Arc;
use tokio::spawn;
use tonic::{Request, Response, Status};

#[tonic::async_trait]
pub trait Remote: Sync + Send + Clone {
    async fn start(&self);
    async fn done(&self);
}

#[derive(Clone)]
pub struct NoopRemote;

#[tonic::async_trait]
impl Remote for NoopRemote {
    async fn start(&self) {}
    async fn done(&self) {}
}

pub struct MyPublisher<R: Remote> {
    accumulator: Arc<Accumulator<Vec<Bytes>>>,
    total_groups: usize,
    remote: R,
}

impl<R: Remote> MyPublisher<R> {
    fn from_protocol<P: Protocol>(protocol: P, remote: R) -> Self {
        MyPublisher {
            accumulator: Arc::new(Accumulator::new(protocol.new_accumulator())),
            total_groups: protocol.num_parties(),
            remote,
        }
    }
}

#[tonic::async_trait]
impl<R: Remote + 'static> Publisher for MyPublisher<R> {
    async fn aggregate_group(
        &self,
        request: Request<AggregateGroupRequest>,
    ) -> Result<Response<AggregateGroupResponse>, Status> {
        let request = request.into_inner();

        let data = expect_field(request.share, "Share")?.data;
        let total_groups = self.total_groups;
        let accumulator = self.accumulator.clone();

        let remote = self.remote.clone();
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
            info!("Publisher finished!");
            trace!("Recovered value len: {:?}", share.len());
            remote.done().await;
        });

        Ok(Response::new(AggregateGroupResponse {}))
    }
}

async fn inner_run<C, F, R, P>(
    config: C,
    protocol: P,
    info: PublisherInfo,
    net: NetConfig,
    remote: R,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store + Sync + Send,
    R: Remote + 'static,
    F: Future<Output = ()> + Send + 'static,
    P: Protocol,
{
    let state = MyPublisher::from_protocol(protocol, remote.clone());
    info!("Publisher starting up.");
    let local_socket_addr = net.local_socket_addr();
    let server_task = tokio::spawn(async move {
        tonic::transport::server::Server::builder()
            .add_service(HealthServer::new(AllGoodHealthServer::default()))
            .add_service(PublisherServer::new(state))
            .serve_with_shutdown(local_socket_addr, shutdown)
            .await
    });

    wait_for_health(format!("http://{}", net.public_addr())).await?;
    trace!("Publisher {:?} healthy and serving.", info);

    let node = Node::new(info.into(), net.public_addr());
    register(&config, node).await?;
    debug!("Registered with config server.");

    let experiment = experiment::read_from_store(&config).await?;
    wait_for_quorum(&config, &experiment).await?;

    // TODO(zjn): should be more in the future
    let start = DateTime::<FixedOffset>::from(Utc::now()) + chrono::Duration::milliseconds(5000);
    info!("Registering experiment start time: {}", start);
    set_start_time(&config, start).await?;
    delay_until(start).await;
    remote.start().await;

    server_task.await??;
    info!("Publisher shutting down.");

    Ok(())
}

pub async fn run<C, R, F>(
    config: C,
    protocol: ProtocolWrapper,
    info: PublisherInfo,
    net: NetConfig,
    remote: R,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error + Sync + Send>>
where
    C: Store + Sync + Send,
    R: Remote + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper::Secure(protocol) => {
            inner_run(config, protocol, info, net, remote, shutdown).await?;
        }
        ProtocolWrapper::Insecure(protocol) => {
            inner_run(config, protocol, info, net, remote, shutdown).await?;
        }
    }
    Ok(())
}
