use crate::proto::{
    self, expect_field,
    worker_server::{Worker, WorkerServer},
    AggregateWorkerRequest, RegisterClientRequest, RegisterClientResponse, Share, UploadRequest,
    UploadResponse, VerifyRequest, VerifyResponse,
};
use crate::{
    bytes::Bytes,
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::{
        accumulator::Accumulator,
        wrapper::{ChannelKeyWrapper, ProtocolWrapper},
        Protocol,
    },
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, WorkerInfo,
    },
};

use futures::prelude::*;
use log::{debug, error, info, trace, warn};
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::sync::Arc;
use tokio::{spawn, sync::watch, task::spawn_blocking};
use tonic::{Request, Response, Status};

mod audit_registry;
mod client_registry;
mod service_registry;

use audit_registry::AuditRegistry;
use client_registry::Registry as ClientRegistry;
use service_registry::{Registry as ServiceRegistry, SharedClient};

type Error = Box<dyn std::error::Error + Sync + Send>;

struct WorkerState<P: Protocol> {
    audit_registry: AuditRegistry<P::AuditShare, P::WriteToken>,
    accumulator: Accumulator<Vec<Bytes>>,
    experiment: Experiment,
    client_registry: ClientRegistry,
    protocol: P,
}

impl<P: Protocol> WorkerState<P> {
    fn from_experiment(experiment: Experiment, protocol: P) -> Self {
        WorkerState {
            audit_registry: AuditRegistry::new(experiment.clients(), experiment.groups()),
            accumulator: Accumulator::new(protocol.new_accumulator()),
            experiment,
            client_registry: ClientRegistry::new(),
            protocol,
        }
    }
}

impl<P> WorkerState<P>
where
    P: Protocol + 'static + Sync + Send + Clone,
    P::WriteToken: Clone + Send + fmt::Debug,
    P::AuditShare: Send,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Send,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: fmt::Debug,
{
    async fn upload(&self, client: &ClientInfo, write_token: P::WriteToken) -> Vec<P::AuditShare> {
        self.audit_registry.init(&client, write_token.clone()).await;

        let protocol = self.protocol.clone();
        let keys = self.experiment.get_keys(); // TODO(zjn): move into WorkerState
        let keys = keys
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<P::ChannelKey>, _>>()
            .unwrap();

        spawn_blocking(move || protocol.gen_audit(&keys, &write_token))
            .await
            .expect("Generating audit should not panic.")
    }

    async fn verify(&self, client: &ClientInfo, share: P::AuditShare) -> Option<Vec<Bytes>> {
        let check_count = self.audit_registry.add(client, share).await;
        trace!(
            "{}/{} shares received for {:?}",
            check_count,
            self.protocol.num_parties(),
            client.clone()
        );
        if check_count < self.protocol.num_parties() {
            return None;
        }
        trace!("Running verification.");

        let (token, shares) = self.audit_registry.drain(client).await;
        let protocol = self.protocol.clone();
        let verify = spawn_blocking(move || protocol.check_audit(shares))
            .await
            .unwrap();
        if !verify {
            warn!("Didn't verify");
            return None;
        }

        let protocol = self.protocol.clone();
        let data = spawn_blocking(move || protocol.to_accumulator(token))
            .await
            .expect("Accepting write token should never fail.");

        if data.len() != self.protocol.num_channels() {
            error!(
                "Invalid data len! {} != {}",
                data.len(),
                self.protocol.message_len()
            );
        } else if data[0].len() != self.protocol.message_len() {
            error!(
                "Invalid data chunk len! {} != {}",
                data.len(),
                self.protocol.message_len()
            );
            debug!("Bad data: {:?}", data[0]);
        }

        let accumulated_clients = self.accumulator.accumulate(data).await;
        let total_clients = self.client_registry.num_clients().await;
        trace!("{}/{} clients", accumulated_clients, total_clients);
        if accumulated_clients == total_clients {
            return Some(self.accumulator.get().await);
        }
        None
    }

    async fn register_client(&self, client: &ClientInfo, shards: Vec<WorkerInfo>) {
        self.client_registry.register_client(client, shards).await;
    }
}

pub struct MyWorker<P: Protocol> {
    start_rx: watch::Receiver<bool>,
    services: ServiceRegistry,
    state: Arc<WorkerState<P>>,
}

impl<P: Protocol> MyWorker<P> {
    fn new(
        start_rx: watch::Receiver<bool>,
        services: ServiceRegistry,
        experiment: Experiment,
        protocol: P,
    ) -> Self {
        let state = WorkerState::from_experiment(experiment, protocol);
        MyWorker {
            start_rx,
            services,
            state: Arc::new(state),
        }
    }

    async fn get_peers(&self, client: &ClientInfo) -> Result<Vec<SharedClient>, Status> {
        self.state
            .client_registry
            .get_peers(client)
            .await?
            .into_iter()
            .map(|info| self.services.get_worker(info))
            .collect()
    }

    fn check_not_started(&self) -> Result<(), Status> {
        let started_lock = self.start_rx.borrow();
        if *started_lock {
            Err(Status::failed_precondition(
                "Client registration after start time.",
            ))
        } else {
            Ok(())
        }
    }
}

#[tonic::async_trait]
impl<P> Worker for MyWorker<P>
where
    P: Protocol + 'static + Sync + Send + Clone,
    P::WriteToken: Clone + TryFrom<proto::WriteToken> + Sync + Send + fmt::Debug,
    <P::WriteToken as TryFrom<proto::WriteToken>>::Error: fmt::Debug + Send,
    P::AuditShare: TryFrom<proto::AuditShare> + Into<proto::AuditShare> + Sync + Send,
    <P::AuditShare as TryFrom<proto::AuditShare>>::Error: fmt::Debug,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Send,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: fmt::Debug,
{
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        let request = request.into_inner();

        let client_id = expect_field(request.client_id, "Client ID")?;
        let client_info = ClientInfo::from(&client_id);
        let write_token = expect_field(request.write_token, "Write Token")?;
        let state = self.state.clone();
        let peers: Vec<SharedClient> = self.get_peers(&client_info).await?;

        spawn(async move {
            let audit_shares = state
                .upload(&client_info, write_token.try_into().unwrap())
                .await;

            for (peer, audit_share) in peers.into_iter().zip(audit_shares.into_iter()) {
                let req = Request::new(VerifyRequest {
                    client_id: Some(client_id.clone()),
                    audit_share: Some(audit_share.into()),
                });
                spawn(async move {
                    peer.lock().await.verify(req).await.unwrap();
                });
            }
            Ok::<_, Status>(())
        });

        Ok(Response::new(UploadResponse {}))
    }

    async fn verify(
        &self,
        request: Request<VerifyRequest>,
    ) -> Result<Response<VerifyResponse>, Status> {
        let request = request.into_inner();

        // TODO(zjn): check which worker this comes from, don't double-insert
        let client_info = ClientInfo::from(&expect_field(request.client_id, "Client ID")?);
        let share = expect_field(request.audit_share, "Audit Share")?;
        let share = share.try_into().unwrap();
        let state = self.state.clone();
        let leader = self.services.get_my_leader();

        spawn(async move {
            if let Some(share) = state.verify(&client_info, share).await {
                let share: Vec<Vec<u8>> = share.into_iter().map(Into::into).collect();
                info!("Forwarding to leader.");
                // trace!("Share: {:?}", share);
                let req = Request::new(AggregateWorkerRequest {
                    share: Some(Share { data: share }),
                });
                leader.lock().await.aggregate_worker(req).await.unwrap();
            }
        });

        Ok(Response::new(VerifyResponse {}))
    }

    async fn register_client(
        &self,
        request: Request<RegisterClientRequest>,
    ) -> Result<Response<RegisterClientResponse>, Status> {
        self.check_not_started()?;

        let request = request.into_inner();
        let client_info = ClientInfo::from(&expect_field(request.client_id, "Client ID")?);
        let shards = request.shards.into_iter().map(WorkerInfo::from).collect();
        self.state.register_client(&client_info, shards).await;

        let reply = RegisterClientResponse {};
        Ok(Response::new(reply))
    }
}

async fn inner_run<C, F, P>(
    config: C,
    experiment: Experiment,
    protocol: P,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Error>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
    P: Protocol + 'static + Sync + Send + Clone,
    P::WriteToken: Clone + TryFrom<proto::WriteToken> + Sync + Send + fmt::Debug,
    <P::WriteToken as TryFrom<proto::WriteToken>>::Error: fmt::Debug + Send,
    P::AuditShare: TryFrom<proto::AuditShare> + Into<proto::AuditShare> + Sync + Send,
    <P::AuditShare as TryFrom<proto::AuditShare>>::Error: fmt::Debug,
    P::ChannelKey: TryFrom<ChannelKeyWrapper> + Send,
    <P::ChannelKey as TryFrom<ChannelKeyWrapper>>::Error: fmt::Debug,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let (start_tx, start_rx) = watch::channel(false);
    let (registry, registry_remote) = ServiceRegistry::new_with_remote();

    let worker = MyWorker::new(start_rx, registry, experiment, protocol);
    let server = tonic::transport::server::Server::builder()
        .add_service(HealthServer::new(AllGoodHealthServer::default()))
        .add_service(WorkerServer::new(worker))
        .serve_with_shutdown(addr, shutdown);

    let server_task = spawn(server);

    wait_for_health(format!("http://{}", addr)).await?;
    trace!("Worker {:?} healthy and serving.", info);
    register(&config, Node::new(info.into(), addr)).await?;

    let start_time = wait_for_start_time_set(&config).await.unwrap();
    registry_remote.init(info, &config).await?;
    spawn(delay_until(start_time).then(|_| async move { start_tx.broadcast(true) }));

    server_task.await??;
    info!("Worker shutting down.");
    Ok(())
}

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    protocol: ProtocolWrapper,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Error>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    match protocol {
        ProtocolWrapper::Secure(protocol) => {
            inner_run(config, experiment, protocol, info, shutdown).await?;
        }
        ProtocolWrapper::Insecure(protocol) => {
            inner_run(config, experiment, protocol, info, shutdown).await?;
        }
    }
    Ok(())
}
