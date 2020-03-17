use crate::proto::{
    expect_field,
    worker_server::{Worker, WorkerServer},
    AggregateWorkerRequest, RegisterClientRequest, RegisterClientResponse, Share, UploadRequest,
    UploadResponse, VerifyRequest, VerifyResponse,
};
use crate::{
    config::store::Store,
    experiment::Experiment,
    net::get_addr,
    protocols::accumulator::Accumulator,
    protocols::{insecure, Bytes, Protocol},
    services::{
        discovery::{register, Node},
        health::{wait_for_health, AllGoodHealthServer, HealthServer},
        quorum::{delay_until, wait_for_start_time_set},
        ClientInfo, WorkerInfo,
    },
};

use futures::prelude::*;
use log::{info, trace, warn};
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

struct WorkerState {
    audit_registry: AuditRegistry<insecure::AuditShare, insecure::WriteToken>,
    accumulator: Accumulator<Vec<Bytes>>,
    experiment: Experiment,
    client_registry: ClientRegistry,
}

impl WorkerState {
    fn from_experiment(experiment: Experiment) -> Self {
        WorkerState {
            audit_registry: AuditRegistry::new(experiment.clients),
            accumulator: Accumulator::new(vec![Default::default(); experiment.channels]),
            experiment,
            client_registry: ClientRegistry::new(),
        }
    }

    async fn upload(
        &self,
        client: &ClientInfo,
        write_token: insecure::WriteToken,
    ) -> Vec<insecure::AuditShare> {
        let protocol = self.experiment.get_protocol();
        let keys = self.experiment.get_keys();

        // Avoid cloning write token by passing it back
        let (audit_shares, write_token) =
            spawn_blocking(move || (protocol.gen_audit(&keys, &write_token), write_token))
                .await
                .expect("Generating audit should not panic.");

        self.audit_registry.init(&client, write_token).await;

        audit_shares
    }

    async fn verify(&self, client: &ClientInfo, share: insecure::AuditShare) -> Option<Vec<Bytes>> {
        let check_count = self.audit_registry.add(client, share).await;
        if check_count < self.experiment.groups as usize {
            return None;
        }

        let (token, shares) = self.audit_registry.drain(client).await;
        let protocol = self.experiment.get_protocol();

        let verify = spawn_blocking(move || protocol.check_audit(shares))
            .await
            .unwrap();

        if !verify {
            warn!("Didn't verify");
            return None;
        }

        let data = spawn_blocking(move || protocol.to_accumulator(token))
            .await
            .expect("Accepting write token should never fail.");

        let accumulated_clients = self.accumulator.accumulate(data).await;
        let total_clients = self.client_registry.num_clients().await;
        if accumulated_clients == total_clients {
            return Some(self.accumulator.get().await);
        }
        None
    }

    async fn register_client(&self, client: &ClientInfo, shards: Vec<WorkerInfo>) {
        self.client_registry.register_client(client, shards).await;
    }
}

pub struct MyWorker {
    start_rx: watch::Receiver<bool>,
    services: ServiceRegistry,
    state: Arc<WorkerState>,
}

impl MyWorker {
    fn new(
        start_rx: watch::Receiver<bool>,
        services: ServiceRegistry,
        experiment: Experiment,
    ) -> Self {
        MyWorker {
            start_rx,
            services,
            state: Arc::new(WorkerState::from_experiment(experiment)),
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
impl Worker for MyWorker {
    async fn upload(
        &self,
        request: Request<UploadRequest>,
    ) -> Result<Response<UploadResponse>, Status> {
        let request = request.into_inner();
        trace!("Request! {:?}", request);

        let client_id = expect_field(request.client_id, "Client ID")?;
        let client_info = ClientInfo::from(&client_id);
        let write_token: insecure::WriteToken =
            expect_field(request.write_token, "Write Token")?.into();
        let state = self.state.clone();
        let peers: Vec<SharedClient> = self.get_peers(&client_info).await?;

        spawn(async move {
            let audit_shares = state.upload(&client_info, write_token).await;

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
        trace!("Request: {:?}", request);

        // TODO(zjn): check which worker this comes from, don't double-insert
        let client_info = ClientInfo::from(&expect_field(request.client_id, "Client ID")?);
        let share = expect_field(request.audit_share, "Audit Share")?;
        let state = self.state.clone();
        let leader = self.services.get_my_leader();

        spawn(async move {
            if let Some(share) = state.verify(&client_info, share.into()).await {
                let share: Vec<Vec<u8>> = share.into_iter().map(Into::into).collect();
                info!("Forwarding to leader: {:?}", share);
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

pub async fn run<C, F>(
    config: C,
    experiment: Experiment,
    info: WorkerInfo,
    shutdown: F,
) -> Result<(), Error>
where
    C: Store,
    F: Future<Output = ()> + Send + 'static,
{
    info!("Worker starting up.");
    let addr = get_addr();

    let (start_tx, start_rx) = watch::channel(false);
    let (registry, registry_remote) = ServiceRegistry::new_with_remote();
    let worker = MyWorker::new(start_rx, registry, experiment);

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
